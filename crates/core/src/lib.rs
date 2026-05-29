pub mod auth;
pub mod ble_discovery;
pub mod ble_sync;
pub mod chat;
pub mod connection_manager;
pub mod crypto;
pub mod db;
pub mod doc_manager;
pub mod gossip_manager;
pub mod group_sync;
pub mod groups;
pub mod key_cache;
pub mod notifier;
pub mod proto;
pub mod resource;
pub mod signing;
pub mod sync;
pub mod sync_protocol;
pub mod topics;
pub mod transport;
pub mod types;
pub mod ws_relay;

/// Re-exported so downstream crates can name peer identities for the gossip
/// bootstrap / join APIs without depending on `iroh` directly.
pub use iroh::EndpointId;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use tokio::sync::Mutex;

use chat::ChatManager;
use connection_manager::ConnectionManager;
use db::Database;
use doc_manager::DocManager;
use gossip_manager::GossipManager;
use groups::GroupManager;
use iroh::endpoint::presets;
use iroh_ble_transport::BleTransport;
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use notifier::ResourceNotifier;
use resource::ResourceRegistry;
use sync::SyncOrchestrator;
use ws_relay::WsRelaySink;

/// Top-level node that ties together the database, document manager, and
/// (optionally) the iroh gossip networking layer.
pub struct OffbeatNode {
    pub doc_manager: Arc<DocManager>,
    pub db: Arc<Database>,
    pub group_manager: Arc<GroupManager>,
    pub chat_manager: Arc<ChatManager>,
    /// Resource registry for tracking syncable resources.
    pub resource_registry: Arc<RwLock<ResourceRegistry>>,
    /// Sync orchestrator for coordinating sync operations.
    pub sync_orchestrator: Arc<SyncOrchestrator>,
    /// Resource notifier for reactive updates.
    pub notifier: Arc<ResourceNotifier>,
    /// Present when the node was created via `new_with_networking`.
    pub gossip_manager: Option<Arc<Mutex<GossipManager>>>,
    /// Present when the node was created via `new_with_networking`.
    pub gossip: Option<Gossip>,
    /// Present when the node was created via `new_with_networking`.
    pub endpoint: Option<iroh::Endpoint>,
    /// Protocol router multiplexing inbound connections across ALPNs (gossip +
    /// `offbeat/sync/1`). Held to keep the accept loop alive.
    pub router: Option<iroh::protocol::Router>,
    /// BLE transport handle, if BLE hardware is available.
    pub ble_transport: Option<Arc<BleTransport>>,
    /// Connection manager for multi-path peer tracking.
    pub connection_manager: Option<Arc<ConnectionManager>>,
    /// WS relay sink — populated after `connect_relay`.
    /// Behind a lock so background watchers (transport status) see updates.
    pub ws_relay: Arc<parking_lot::RwLock<Option<Arc<WsRelaySink>>>>,
    /// Cached festival public keys (festival_id → 32-byte Ed25519 key).
    pub festival_public_keys: HashMap<String, [u8; 32]>,
}

impl OffbeatNode {
    /// Open (or create) the database at `db_path` and initialise the node
    /// **without** networking.  Used by tests and the bridge when networking
    /// is not yet needed.
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        let db = Arc::new(Database::new(db_path)?);
        let doc_manager = Arc::new(DocManager::new(db.clone()));
        let group_manager = Arc::new(GroupManager::new(db.clone(), doc_manager.clone()));
        let chat_manager = Arc::new(ChatManager::new(db.clone(), doc_manager.clone()));
        let resource_registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        let notifier = ResourceNotifier::new_arc();
        let sync_orchestrator = Arc::new(SyncOrchestrator::new(
            resource_registry.clone(),
            doc_manager.clone(),
            chat_manager.clone(),
            db.clone(),
            notifier.clone(),
        ));
        Ok(Self {
            doc_manager,
            db,
            group_manager,
            chat_manager,
            resource_registry,
            sync_orchestrator,
            notifier,
            gossip_manager: None,
            gossip: None,
            endpoint: None,
            router: None,
            ble_transport: None,
            connection_manager: None,
            ws_relay: Arc::new(parking_lot::RwLock::new(None)),
            festival_public_keys: HashMap::new(),
        })
    }

    /// Create an in-memory node (useful for tests).
    pub fn new_in_memory() -> anyhow::Result<Self> {
        let db = Arc::new(Database::new_in_memory()?);
        let doc_manager = Arc::new(DocManager::new(db.clone()));
        let group_manager = Arc::new(GroupManager::new(db.clone(), doc_manager.clone()));
        let chat_manager = Arc::new(ChatManager::new(db.clone(), doc_manager.clone()));
        let resource_registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        let notifier = ResourceNotifier::new_arc();
        let sync_orchestrator = Arc::new(SyncOrchestrator::new(
            resource_registry.clone(),
            doc_manager.clone(),
            chat_manager.clone(),
            db.clone(),
            notifier.clone(),
        ));
        Ok(Self {
            doc_manager,
            db,
            group_manager,
            chat_manager,
            resource_registry,
            sync_orchestrator,
            notifier,
            gossip_manager: None,
            gossip: None,
            endpoint: None,
            router: None,
            ble_transport: None,
            connection_manager: None,
            ws_relay: Arc::new(parking_lot::RwLock::new(None)),
            festival_public_keys: HashMap::new(),
        })
    }

    /// Create a node with a full iroh networking stack.
    ///
    /// This binds an iroh `Endpoint`, spawns the gossip actor (using the
    /// standard `GOSSIP_ALPN`), and wires up a `GossipManager`.
    pub async fn new_with_networking(db_path: &Path) -> anyhow::Result<Self> {
        let db = Arc::new(Database::new(db_path)?);
        let doc_manager = Arc::new(DocManager::new(db.clone()));
        let group_manager = Arc::new(GroupManager::new(db.clone(), doc_manager.clone()));
        let chat_manager = Arc::new(ChatManager::new(db.clone(), doc_manager.clone()));
        let resource_registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        let notifier = ResourceNotifier::new_arc();
        let mut sync_orchestrator = Arc::new(SyncOrchestrator::new(
            resource_registry.clone(),
            doc_manager.clone(),
            chat_manager.clone(),
            db.clone(),
            notifier.clone(),
        ));

        // Load a persisted iroh secret key so the EndpointId is stable across
        // restarts.  If none exists yet, generate a fresh one and persist it.
        let secret_key = match db.load_iroh_secret_key()? {
            Some(key) => {
                tracing::info!("loaded persisted iroh secret key");
                key
            }
            None => {
                let key = iroh::SecretKey::generate();
                db.save_iroh_secret_key(&key)?;
                tracing::info!("generated and persisted new iroh secret key");
                key
            }
        };
        // Capture the public key before secret_key is moved into the builder.
        let own_endpoint_id = secret_key.public();
        let ble_transport = transport::ble::try_build_ble(own_endpoint_id).await;

        let mut builder = iroh::Endpoint::builder(presets::N0)
            .secret_key(secret_key)
            .alpns(vec![GOSSIP_ALPN.to_vec(), sync_protocol::SYNC_ALPN.to_vec()]);

        if let Some(ref ble) = ble_transport {
            builder = builder
                .hooks(ble.dedup_hook())
                .add_custom_transport(ble.as_custom_transport())
                .address_lookup(ble.address_lookup());
        }

        let endpoint = builder.bind().await?;

        // Create the connection manager with the hex-encoded endpoint ID,
        // backed by the durable peer directory for offline cold-start.
        let connection_manager = Arc::new(ConnectionManager::new_with_db(
            own_endpoint_id.to_string(),
            db.clone(),
        ));

        // Spawn the gossip actor; it takes ownership of a clone of the endpoint.
        let gossip = Gossip::builder().spawn(endpoint.clone());

        // Route inbound connections by ALPN: gossip's overlay traffic to the
        // gossip actor, and offbeat/sync/1 catch-up to our SyncProtocol. This
        // is also what gives gossip an accept loop at all — without a router,
        // inbound gossip dials were never handled.
        let router = iroh::protocol::Router::builder(endpoint.clone())
            .accept(GOSSIP_ALPN, gossip.clone())
            .accept(
                sync_protocol::SYNC_ALPN,
                sync_protocol::SyncProtocol::new(doc_manager.clone()),
            )
            .spawn();

        let gossip_manager = Arc::new(Mutex::new(GossipManager::new(gossip.clone())));

        // Wire gossip manager into sync orchestrator for neighbor counts
        Arc::get_mut(&mut sync_orchestrator)
            .expect("sync_orchestrator not yet shared")
            .set_gossip_manager(gossip_manager.clone());

        Ok(Self {
            doc_manager,
            db,
            group_manager,
            chat_manager,
            resource_registry,
            sync_orchestrator,
            notifier,
            gossip_manager: Some(gossip_manager),
            gossip: Some(gossip),
            endpoint: Some(endpoint),
            router: Some(router),
            ble_transport,
            connection_manager: Some(connection_manager),
            ws_relay: Arc::new(parking_lot::RwLock::new(None)),
            festival_public_keys: HashMap::new(),
        })
    }
}
