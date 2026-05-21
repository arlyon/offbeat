pub mod auth;
pub mod chat;
pub mod crypto;
pub mod db;
pub mod doc_manager;
pub mod gossip_manager;
pub mod groups;
pub mod signing;
pub mod topics;
pub mod transport;
pub mod types;
pub mod ws_relay;

use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use std::collections::HashMap;

use chat::ChatManager;
use db::Database;
use doc_manager::DocManager;
use gossip_manager::GossipManager;
use groups::GroupManager;
use iroh::endpoint::presets;
use iroh_gossip::net::{Gossip, GOSSIP_ALPN};
use ws_relay::WsRelaySink;

/// Top-level node that ties together the database, document manager, and
/// (optionally) the iroh gossip networking layer.
pub struct OffbeatNode {
    pub doc_manager: Arc<Mutex<DocManager>>,
    pub db: Arc<Database>,
    pub group_manager: Arc<GroupManager>,
    pub chat_manager: Arc<ChatManager>,
    /// Present when the node was created via `new_with_networking`.
    pub gossip_manager: Option<Arc<Mutex<GossipManager>>>,
    /// Present when the node was created via `new_with_networking`.
    pub gossip: Option<Gossip>,
    /// Present when the node was created via `new_with_networking`.
    pub endpoint: Option<iroh::Endpoint>,
    /// WS relay sink — present after `connect_relay`.
    pub ws_relay: Option<Arc<WsRelaySink>>,
    /// Cached festival public keys (festival_id → 32-byte Ed25519 key).
    pub festival_public_keys: HashMap<String, [u8; 32]>,
}

impl OffbeatNode {
    /// Open (or create) the database at `db_path` and initialise the node
    /// **without** networking.  Used by tests and the bridge when networking
    /// is not yet needed.
    pub fn new(db_path: &Path) -> anyhow::Result<Self> {
        let db = Arc::new(Database::new(db_path)?);
        let doc_manager = Arc::new(Mutex::new(DocManager::new(db.clone())));
        let group_manager = Arc::new(GroupManager::new(db.clone(), doc_manager.clone()));
        let chat_manager = Arc::new(ChatManager::new(db.clone(), doc_manager.clone()));
        Ok(Self {
            doc_manager,
            db,
            group_manager,
            chat_manager,
            gossip_manager: None,
            gossip: None,
            endpoint: None,
            ws_relay: None,
            festival_public_keys: HashMap::new(),
        })
    }

    /// Create an in-memory node (useful for tests).
    pub fn new_in_memory() -> anyhow::Result<Self> {
        let db = Arc::new(Database::new_in_memory()?);
        let doc_manager = Arc::new(Mutex::new(DocManager::new(db.clone())));
        let group_manager = Arc::new(GroupManager::new(db.clone(), doc_manager.clone()));
        let chat_manager = Arc::new(ChatManager::new(db.clone(), doc_manager.clone()));
        Ok(Self {
            doc_manager,
            db,
            group_manager,
            chat_manager,
            gossip_manager: None,
            gossip: None,
            endpoint: None,
            ws_relay: None,
            festival_public_keys: HashMap::new(),
        })
    }

    /// Create a node with a full iroh networking stack.
    ///
    /// This binds an iroh `Endpoint`, spawns the gossip actor (using the
    /// standard `GOSSIP_ALPN`), and wires up a `GossipManager`.
    pub async fn new_with_networking(db_path: &Path) -> anyhow::Result<Self> {
        let db = Arc::new(Database::new(db_path)?);
        let doc_manager = Arc::new(Mutex::new(DocManager::new(db.clone())));
        let group_manager = Arc::new(GroupManager::new(db.clone(), doc_manager.clone()));
        let chat_manager = Arc::new(ChatManager::new(db.clone(), doc_manager.clone()));

        // Bind an iroh endpoint, accepting gossip connections.
        let endpoint = iroh::Endpoint::builder(presets::N0)
            .alpns(vec![GOSSIP_ALPN.to_vec()])
            .bind()
            .await?;

        // Spawn the gossip actor; it takes ownership of a clone of the endpoint.
        let gossip = Gossip::builder().spawn(endpoint.clone());

        let gossip_manager = Arc::new(Mutex::new(GossipManager::new(
            gossip.clone(),
            Arc::clone(&doc_manager),
            Arc::clone(&db),
        )));

        Ok(Self {
            doc_manager,
            db,
            group_manager,
            chat_manager,
            gossip_manager: Some(gossip_manager),
            gossip: Some(gossip),
            endpoint: Some(endpoint),
            ws_relay: None,
            festival_public_keys: HashMap::new(),
        })
    }
}
