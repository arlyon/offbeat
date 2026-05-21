//! SyncOrchestrator — unified sync coordination for all resources.
//!
//! This module provides:
//! - `PeerConnection` trait — abstract interface for WS relay and iroh-gossip
//! - `SyncOrchestrator` — coordinates sync for all registered resources
//! - `SyncReport` — summary of sync operations performed

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};

use tokio::sync::Mutex;

use crate::chat::ChatManager;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::gossip_manager::{dispatch_message, GossipMessage, GossipWireMessage};
use crate::resource::{Priority, ResourceKind, ResourceRegistry};
use crate::types::ChatMessage;

// ---------------------------------------------------------------------------
// ChatStateVector — per-writer high water marks for chat sync
// ---------------------------------------------------------------------------

/// Per-writer sequence numbers for chat catch-up.
#[derive(Debug, Clone, Default)]
pub struct ChatStateVector {
    /// Map of writer_id → highest seen sequence number.
    pub writers: HashMap<String, u64>,
}

impl ChatStateVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build from existing chat messages.
    pub fn from_messages(messages: &[ChatMessage]) -> Self {
        let mut writers = HashMap::new();
        for msg in messages {
            let entry = writers.entry(msg.user_id.clone()).or_insert(0u64);
            if msg.writer_seq > *entry {
                *entry = msg.writer_seq;
            }
        }
        Self { writers }
    }

    /// Encode as JSON for wire transport.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(&self.writers).unwrap_or_default()
    }

    /// Decode from JSON bytes.
    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        let writers: HashMap<String, u64> = serde_json::from_slice(bytes)?;
        Ok(Self { writers })
    }
}

// ---------------------------------------------------------------------------
// SyncReport — summary of sync operations
// ---------------------------------------------------------------------------

/// Summary of sync operations performed with a peer.
#[derive(Debug, Clone, Default)]
pub struct SyncReport {
    /// Number of resources synced.
    pub resources_synced: u32,
    /// Number of CRDT updates applied.
    pub crdt_updates_applied: u32,
    /// Number of chat messages received.
    pub chat_messages_received: u32,
    /// Resources that failed to sync.
    pub failed: Vec<String>,
}

// ---------------------------------------------------------------------------
// PeerConnection — abstract peer interface
// ---------------------------------------------------------------------------

/// Abstract peer connection — works for both WS relay and iroh-gossip.
///
/// Implementations must be Send + Sync to work with async code.
pub trait PeerConnection: Send + Sync {
    /// Subscribe to a set of topic strings.
    fn subscribe(&self, topics: Vec<String>) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Perform state vector exchange for a CRDT doc.
    /// Sends our SV, receives updates we're missing.
    fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Request chat messages since our state vector.
    fn chat_catchup(
        &self,
        topic: &str,
        sv: &ChatStateVector,
        limit: u32,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Broadcast data on a topic.
    fn broadcast(&self, topic: &str, data: &[u8]) -> impl Future<Output = anyhow::Result<()>> + Send;
}

// ---------------------------------------------------------------------------
// SyncOrchestrator
// ---------------------------------------------------------------------------

/// Orchestrates sync for all registered resources in priority order.
pub struct SyncOrchestrator {
    registry: Arc<RwLock<ResourceRegistry>>,
    doc_manager: Arc<Mutex<DocManager>>,
    #[allow(dead_code)]
    chat_manager: Arc<ChatManager>,
    db: Arc<Database>,
    /// Cached festival public keys for signature verification.
    festival_public_keys: Arc<RwLock<HashMap<String, [u8; 32]>>>,
}

impl SyncOrchestrator {
    /// Create a new SyncOrchestrator.
    pub fn new(
        registry: Arc<RwLock<ResourceRegistry>>,
        doc_manager: Arc<Mutex<DocManager>>,
        chat_manager: Arc<ChatManager>,
        db: Arc<Database>,
    ) -> Self {
        Self {
            registry,
            doc_manager,
            chat_manager,
            db,
            festival_public_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Cache a festival's Ed25519 public key.
    pub fn set_festival_public_key(&self, festival_id: &str, public_key: [u8; 32]) {
        if let Ok(mut map) = self.festival_public_keys.write() {
            map.insert(festival_id.to_string(), public_key);
        }
    }

    /// Get a festival's public key if cached.
    pub fn get_festival_public_key(&self, festival_id: &str) -> Option<[u8; 32]> {
        self.festival_public_keys.read().ok()?.get(festival_id).copied()
    }

    /// Run subscribe→catch-up→live for all resources with a peer.
    ///
    /// Resources are synced in priority order (CRITICAL first, then HIGH, etc.).
    pub async fn sync_with_peer<P: PeerConnection>(&self, peer: &P) -> anyhow::Result<SyncReport> {
        let mut report = SyncReport::default();

        // Get resources sorted by priority
        let resources: Vec<(String, ResourceKind, String, Priority)> = {
            let reg = self.registry.read().map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
            reg.by_priority()
                .iter()
                .map(|r| {
                    (
                        r.id().to_string(),
                        r.kind(),
                        r.topic_string(),
                        r.priority(),
                    )
                })
                .collect()
        };

        // Subscribe to all topics first
        let topics: Vec<String> = resources.iter().map(|(_, _, t, _)| t.clone()).collect();
        if !topics.is_empty() {
            peer.subscribe(topics).await?;
        }

        // Then sync each resource
        for (id, kind, topic, _priority) in resources {
            match self.sync_resource_impl(&id, kind, &topic, peer).await {
                Ok((crdt_count, chat_count)) => {
                    report.resources_synced += 1;
                    report.crdt_updates_applied += crdt_count;
                    report.chat_messages_received += chat_count;
                }
                Err(e) => {
                    tracing::warn!("sync_resource {id} failed: {e}");
                    report.failed.push(id);
                }
            }
        }

        Ok(report)
    }

    /// Sync a single resource by ID.
    pub async fn sync_resource<P: PeerConnection>(
        &self,
        resource_id: &str,
        peer: &P,
    ) -> anyhow::Result<()> {
        let (kind, topic) = {
            let reg = self.registry.read().map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
            let r = reg
                .get(resource_id)
                .ok_or_else(|| anyhow::anyhow!("resource not found: {resource_id}"))?;
            (r.kind(), r.topic_string())
        };

        self.sync_resource_impl(resource_id, kind, &topic, peer)
            .await
            .map(|_| ())
    }

    /// Internal sync implementation.
    async fn sync_resource_impl<P: PeerConnection>(
        &self,
        resource_id: &str,
        kind: ResourceKind,
        topic: &str,
        peer: &P,
    ) -> anyhow::Result<(u32, u32)> {
        match kind {
            ResourceKind::CrdtDoc => {
                // Send state vector, peer will respond with diff
                let sv = {
                    let mut dm = self.doc_manager.lock().await;
                    dm.get_or_create(resource_id);
                    dm.get_state_vector(resource_id)?
                };
                peer.sv_exchange(resource_id, &sv).await?;
                // The actual update will arrive via handle_incoming
                Ok((1, 0))
            }
            ResourceKind::AppendLog => {
                // Build chat state vector from existing messages
                let messages = self.db.get_chat_messages(topic, 1000, 0)?;
                let csv = ChatStateVector::from_messages(&messages);
                peer.chat_catchup(topic, &csv, 100).await?;
                // Messages will arrive via handle_incoming
                Ok((0, 1))
            }
        }
    }

    /// Handle an incoming gossip message, routing to the correct handler.
    ///
    /// This is called when a message arrives from either WS relay or iroh-gossip.
    pub async fn handle_incoming(
        &self,
        topic: &str,
        wire: &GossipWireMessage,
    ) -> anyhow::Result<()> {
        // Determine festival ID from topic for public key lookup
        let festival_pk = self.extract_festival_public_key(topic);

        // Decode wire message to GossipMessage
        let gossip_msg = self.decode_wire_message(wire).await?;
        let Some(msg) = gossip_msg else {
            return Ok(()); // Message type not handled or missing key
        };

        // Dispatch to handlers
        let mut dm = self.doc_manager.lock().await;
        let pk = festival_pk.unwrap_or([0u8; 32]);
        dispatch_message(&mut dm, &self.db, msg, &pk)
    }

    /// Extract festival public key from topic string if applicable.
    fn extract_festival_public_key(&self, topic: &str) -> Option<[u8; 32]> {
        // Topics like "festival/{id}/state" or "offbeat/{id}/state"
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() >= 2 && (parts[0] == "festival" || parts[0] == "offbeat") {
            return self.get_festival_public_key(parts[1]);
        }
        None
    }

    /// Decode a wire message, performing DB lookups for group keys.
    async fn decode_wire_message(
        &self,
        wire: &GossipWireMessage,
    ) -> anyhow::Result<Option<GossipMessage>> {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        match wire.kind.as_str() {
            "festival_update" => {
                let signed_update: crate::types::SignedUpdate =
                    serde_json::from_str(&wire.payload)?;
                Ok(Some(GossipMessage::FestivalUpdate {
                    doc_id: wire
                        .doc_id
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("festival_update missing doc_id"))?,
                    signed_update,
                }))
            }

            "chat" => {
                let chat: ChatMessage = serde_json::from_str(&wire.payload)?;
                Ok(Some(GossipMessage::Chat(chat)))
            }

            "group_update" => {
                let key_id = wire
                    .group_key_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("group_update missing group_key_id"))?;

                let db = Arc::clone(&self.db);
                let key_id_owned = key_id.to_string();
                let group_key = tokio::task::spawn_blocking(move || db.load_group_key(&key_id_owned))
                    .await??
                    .ok_or_else(|| anyhow::anyhow!("group_update: unknown group key {key_id}"))?;

                let encrypted = b64
                    .decode(&wire.payload)
                    .map_err(|e| anyhow::anyhow!("group_update: base64 decode: {e}"))?;

                Ok(Some(GossipMessage::GroupUpdate {
                    doc_id: wire
                        .doc_id
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("group_update missing doc_id"))?,
                    encrypted,
                    group_key,
                }))
            }

            "encrypted_chat" => {
                let key_id = wire
                    .group_key_id
                    .as_deref()
                    .ok_or_else(|| anyhow::anyhow!("encrypted_chat missing group_key_id"))?;

                let db = Arc::clone(&self.db);
                let key_id_owned = key_id.to_string();
                let group_key = tokio::task::spawn_blocking(move || db.load_group_key(&key_id_owned))
                    .await??
                    .ok_or_else(|| anyhow::anyhow!("encrypted_chat: unknown group key {key_id}"))?;

                let encrypted = b64
                    .decode(&wire.payload)
                    .map_err(|e| anyhow::anyhow!("encrypted_chat: base64 decode: {e}"))?;

                Ok(Some(GossipMessage::EncryptedChat { group_key, encrypted }))
            }

            "sync_response" | "sync_update" => {
                let key_id = wire.group_key_id.as_deref();
                if key_id.is_none() {
                    tracing::warn!("sync message without group_key_id; skipping");
                    return Ok(None);
                }
                let key_id = key_id.unwrap();

                let db = Arc::clone(&self.db);
                let key_id_owned = key_id.to_string();
                let group_key = tokio::task::spawn_blocking(move || db.load_group_key(&key_id_owned))
                    .await??
                    .ok_or_else(|| anyhow::anyhow!("sync message: unknown group key {key_id}"))?;

                let encrypted_diff = b64
                    .decode(&wire.payload)
                    .map_err(|e| anyhow::anyhow!("sync message: base64 decode: {e}"))?;

                let doc_id = wire
                    .doc_id
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("sync message missing doc_id"))?;

                if wire.kind == "sync_response" {
                    Ok(Some(GossipMessage::SyncResponse {
                        doc_id,
                        encrypted_diff,
                        group_key,
                    }))
                } else {
                    Ok(Some(GossipMessage::SyncUpdate {
                        doc_id,
                        encrypted_diff,
                        group_key,
                    }))
                }
            }

            "sync_request" => {
                // SyncRequest needs special handling — we need to respond
                tracing::debug!("sync_request received; handled at higher level");
                Ok(None)
            }

            other => {
                tracing::warn!("unknown message kind: {other}; skipping");
                Ok(None)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    // -----------------------------------------------------------------------
    // ChatStateVector tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_chat_state_vector_from_messages() {
        let messages = vec![
            ChatMessage {
                id: "m1".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "hello".to_string(),
                topic: "test".to_string(),
                stage_id: None,
                timestamp: "2026-01-01".to_string(),
                writer_seq: 1,
            },
            ChatMessage {
                id: "m2".to_string(),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: "world".to_string(),
                topic: "test".to_string(),
                stage_id: None,
                timestamp: "2026-01-01".to_string(),
                writer_seq: 2,
            },
            ChatMessage {
                id: "m3".to_string(),
                user_id: "bob".to_string(),
                display_name: "Bob".to_string(),
                text: "hi".to_string(),
                topic: "test".to_string(),
                stage_id: None,
                timestamp: "2026-01-01".to_string(),
                writer_seq: 1,
            },
        ];

        let csv = ChatStateVector::from_messages(&messages);
        assert_eq!(csv.writers.get("alice"), Some(&2));
        assert_eq!(csv.writers.get("bob"), Some(&1));
    }

    #[test]
    fn test_chat_state_vector_encode_decode() {
        let mut csv = ChatStateVector::new();
        csv.writers.insert("alice".to_string(), 5);
        csv.writers.insert("bob".to_string(), 3);

        let encoded = csv.encode();
        let decoded = ChatStateVector::decode(&encoded).unwrap();

        assert_eq!(decoded.writers.get("alice"), Some(&5));
        assert_eq!(decoded.writers.get("bob"), Some(&3));
    }

    #[test]
    fn test_sync_report_default() {
        let report = SyncReport::default();
        assert_eq!(report.resources_synced, 0);
        assert_eq!(report.crdt_updates_applied, 0);
        assert_eq!(report.chat_messages_received, 0);
        assert!(report.failed.is_empty());
    }

    // -----------------------------------------------------------------------
    // SyncOrchestrator tests
    // -----------------------------------------------------------------------

    fn create_orchestrator() -> SyncOrchestrator {
        let db = test_db();
        let doc_manager = Arc::new(Mutex::new(crate::doc_manager::DocManager::new(db.clone())));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(db.clone(), doc_manager.clone()));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        SyncOrchestrator::new(registry, doc_manager, chat_manager, db)
    }

    #[test]
    fn test_orchestrator_festival_public_key_cache() {
        let orch = create_orchestrator();

        // Initially no key
        assert!(orch.get_festival_public_key("fest1").is_none());

        // Set and retrieve
        let key = [42u8; 32];
        orch.set_festival_public_key("fest1", key);
        assert_eq!(orch.get_festival_public_key("fest1"), Some(key));

        // Different festival still none
        assert!(orch.get_festival_public_key("fest2").is_none());
    }

    #[test]
    fn test_extract_festival_public_key_from_topic() {
        let orch = create_orchestrator();

        let key = [99u8; 32];
        orch.set_festival_public_key("glastonbury", key);

        // Festival topic format
        assert_eq!(
            orch.extract_festival_public_key("festival/glastonbury/state"),
            Some(key)
        );

        // Offbeat topic format
        assert_eq!(
            orch.extract_festival_public_key("offbeat/glastonbury/chat"),
            Some(key)
        );

        // Unknown festival
        assert!(orch.extract_festival_public_key("festival/unknown/state").is_none());

        // Non-festival topic
        assert!(orch.extract_festival_public_key("group/abc123/state").is_none());

        // Malformed topic
        assert!(orch.extract_festival_public_key("invalid").is_none());
    }

    #[tokio::test]
    async fn test_decode_wire_message_festival_update() {
        let orch = create_orchestrator();

        let signed = crate::types::SignedUpdate {
            update: "dXBkYXRl".to_string(), // base64 "update"
            author: "organizer".to_string(),
            signature: "c2ln".to_string(), // base64 "sig"
        };

        let wire = GossipWireMessage {
            kind: "festival_update".to_string(),
            payload: serde_json::to_string(&signed).unwrap(),
            doc_id: Some("festival/test/state".to_string()),
            group_key_id: None,
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(matches!(result, Some(GossipMessage::FestivalUpdate { .. })));

        if let Some(GossipMessage::FestivalUpdate { doc_id, signed_update }) = result {
            assert_eq!(doc_id, "festival/test/state");
            assert_eq!(signed_update.author, "organizer");
        }
    }

    #[tokio::test]
    async fn test_decode_wire_message_chat() {
        let orch = create_orchestrator();

        let chat = ChatMessage {
            id: "msg1".to_string(),
            user_id: "user1".to_string(),
            display_name: "User".to_string(),
            text: "hello".to_string(),
            topic: "test".to_string(),
            stage_id: None,
            timestamp: "2026-01-01".to_string(),
            writer_seq: 1,
        };

        let wire = GossipWireMessage {
            kind: "chat".to_string(),
            payload: serde_json::to_string(&chat).unwrap(),
            doc_id: None,
            group_key_id: None,
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(matches!(result, Some(GossipMessage::Chat(_))));

        if let Some(GossipMessage::Chat(msg)) = result {
            assert_eq!(msg.id, "msg1");
            assert_eq!(msg.text, "hello");
        }
    }

    #[tokio::test]
    async fn test_decode_wire_message_unknown_kind() {
        let orch = create_orchestrator();

        let wire = GossipWireMessage {
            kind: "totally_unknown".to_string(),
            payload: "{}".to_string(),
            doc_id: None,
            group_key_id: None,
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_decode_wire_message_sync_request_skipped() {
        let orch = create_orchestrator();

        let wire = GossipWireMessage {
            kind: "sync_request".to_string(),
            payload: "{}".to_string(),
            doc_id: Some("doc".to_string()),
            group_key_id: Some("key".to_string()),
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        // sync_request is handled at a higher level
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_decode_wire_message_group_update_missing_key() {
        let orch = create_orchestrator();

        let wire = GossipWireMessage {
            kind: "group_update".to_string(),
            payload: "encrypted_data".to_string(),
            doc_id: Some("group/test".to_string()),
            group_key_id: None, // Missing!
        };

        let result = orch.decode_wire_message(&wire).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_wire_message_encrypted_chat_unknown_group() {
        let orch = create_orchestrator();

        let wire = GossipWireMessage {
            kind: "encrypted_chat".to_string(),
            payload: "ZW5jcnlwdGVk".to_string(), // base64 "encrypted"
            doc_id: None,
            group_key_id: Some("unknown_group_id".to_string()),
        };

        // Should fail because group key is not in DB
        let result = orch.decode_wire_message(&wire).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_wire_message_sync_without_group_key() {
        let orch = create_orchestrator();

        let wire = GossipWireMessage {
            kind: "sync_response".to_string(),
            payload: "ZGF0YQ==".to_string(),
            doc_id: Some("doc".to_string()),
            group_key_id: None, // Missing
        };

        // Should return None (skipped)
        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // Mock PeerConnection for testing sync_with_peer
    // -----------------------------------------------------------------------

    struct MockPeer {
        subscribed: std::sync::Mutex<Vec<String>>,
        sv_exchanges: std::sync::Mutex<Vec<String>>,
    }

    impl MockPeer {
        fn new() -> Self {
            Self {
                subscribed: std::sync::Mutex::new(vec![]),
                sv_exchanges: std::sync::Mutex::new(vec![]),
            }
        }
    }

    impl PeerConnection for MockPeer {
        async fn subscribe(&self, topics: Vec<String>) -> anyhow::Result<()> {
            self.subscribed.lock().unwrap().extend(topics);
            Ok(())
        }

        async fn sv_exchange(&self, doc_id: &str, _sv: &[u8]) -> anyhow::Result<()> {
            self.sv_exchanges.lock().unwrap().push(doc_id.to_string());
            Ok(())
        }

        async fn chat_catchup(
            &self,
            _topic: &str,
            _sv: &ChatStateVector,
            _limit: u32,
        ) -> anyhow::Result<()> {
            Ok(())
        }

        async fn broadcast(&self, _topic: &str, _data: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_sync_with_peer_empty_registry() {
        let orch = create_orchestrator();
        let peer = MockPeer::new();

        let report = orch.sync_with_peer(&peer).await.unwrap();

        // Empty registry = nothing to sync
        assert_eq!(report.resources_synced, 0);
        assert!(report.failed.is_empty());
        assert!(peer.subscribed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_sync_with_peer_with_resources() {
        use crate::resource::{FestivalState, StageChat};

        let db = test_db();
        let doc_manager = Arc::new(Mutex::new(crate::doc_manager::DocManager::new(db.clone())));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(db.clone(), doc_manager.clone()));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));

        let dummy_key = [0u8; 32];

        // Register some resources
        {
            let mut reg = registry.write().unwrap();
            reg.register(Box::new(FestivalState::new("fest1", dummy_key)));
            reg.register(Box::new(StageChat::new("fest1", "main-stage", dummy_key)));
        }

        let orch = SyncOrchestrator::new(registry.clone(), doc_manager, chat_manager, db);
        let peer = MockPeer::new();

        let report = orch.sync_with_peer(&peer).await.unwrap();

        // Should have synced 2 resources
        assert_eq!(report.resources_synced, 2);
        assert!(report.failed.is_empty());

        // Check subscriptions
        let subs = peer.subscribed.lock().unwrap();
        assert_eq!(subs.len(), 2);

        // Check sv_exchanges (only for CRDT docs)
        let exchanges = peer.sv_exchanges.lock().unwrap();
        assert_eq!(exchanges.len(), 1); // FestivalState is CrdtDoc
        assert!(exchanges[0].contains("fest1"));
    }

    #[tokio::test]
    async fn test_sync_resource_not_found() {
        let orch = create_orchestrator();
        let peer = MockPeer::new();

        let result = orch.sync_resource("nonexistent", &peer).await;
        assert!(result.is_err());
    }

    // Helper to create orchestrator with a group key in DB
    fn create_orchestrator_with_group_key(group_id: &str, group_key: [u8; 32]) -> SyncOrchestrator {
        let db = test_db();
        db.save_group(group_id, "test-fest", "Test Group", &group_key).unwrap();
        let doc_manager = Arc::new(Mutex::new(crate::doc_manager::DocManager::new(db.clone())));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(db.clone(), doc_manager.clone()));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        SyncOrchestrator::new(registry, doc_manager, chat_manager, db)
    }

    #[tokio::test]
    async fn test_decode_wire_message_group_update_with_valid_key() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted = crate::crypto::encrypt(&group_key, b"test data").unwrap();

        let wire = GossipWireMessage {
            kind: "group_update".to_string(),
            payload: b64.encode(&encrypted),
            doc_id: Some(format!("group/{group_id}")),
            group_key_id: Some(group_id.clone()),
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(matches!(result, Some(GossipMessage::GroupUpdate { .. })));

        if let Some(GossipMessage::GroupUpdate { doc_id, encrypted: enc, group_key: key }) = result {
            assert_eq!(doc_id, format!("group/{group_id}"));
            assert_eq!(key, group_key);
            assert_eq!(enc, encrypted);
        }
    }

    #[tokio::test]
    async fn test_decode_wire_message_encrypted_chat_with_valid_key() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted = crate::crypto::encrypt(&group_key, b"chat message").unwrap();

        let wire = GossipWireMessage {
            kind: "encrypted_chat".to_string(),
            payload: b64.encode(&encrypted),
            doc_id: None,
            group_key_id: Some(group_id.clone()),
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(matches!(result, Some(GossipMessage::EncryptedChat { .. })));

        if let Some(GossipMessage::EncryptedChat { group_key: key, encrypted: enc }) = result {
            assert_eq!(key, group_key);
            assert_eq!(enc, encrypted);
        }
    }

    #[tokio::test]
    async fn test_decode_wire_message_sync_response_with_valid_key() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted_diff = crate::crypto::encrypt(&group_key, b"diff data").unwrap();

        let wire = GossipWireMessage {
            kind: "sync_response".to_string(),
            payload: b64.encode(&encrypted_diff),
            doc_id: Some("group/test-doc".to_string()),
            group_key_id: Some(group_id.clone()),
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(matches!(result, Some(GossipMessage::SyncResponse { .. })));

        if let Some(GossipMessage::SyncResponse { doc_id, encrypted_diff: enc, group_key: key }) = result {
            assert_eq!(doc_id, "group/test-doc");
            assert_eq!(key, group_key);
            assert_eq!(enc, encrypted_diff);
        }
    }

    #[tokio::test]
    async fn test_decode_wire_message_sync_update_with_valid_key() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted_diff = crate::crypto::encrypt(&group_key, b"update data").unwrap();

        let wire = GossipWireMessage {
            kind: "sync_update".to_string(),
            payload: b64.encode(&encrypted_diff),
            doc_id: Some("group/test-doc".to_string()),
            group_key_id: Some(group_id.clone()),
        };

        let result = orch.decode_wire_message(&wire).await.unwrap();
        assert!(matches!(result, Some(GossipMessage::SyncUpdate { .. })));

        if let Some(GossipMessage::SyncUpdate { doc_id, encrypted_diff: enc, group_key: key }) = result {
            assert_eq!(doc_id, "group/test-doc");
            assert_eq!(key, group_key);
            assert_eq!(enc, encrypted_diff);
        }
    }

    #[tokio::test]
    async fn test_decode_wire_message_festival_update_missing_doc_id() {
        let orch = create_orchestrator();

        let signed = crate::types::SignedUpdate {
            update: "dXBkYXRl".to_string(),
            author: "org".to_string(),
            signature: "c2ln".to_string(),
        };

        let wire = GossipWireMessage {
            kind: "festival_update".to_string(),
            payload: serde_json::to_string(&signed).unwrap(),
            doc_id: None, // Missing!
            group_key_id: None,
        };

        let result = orch.decode_wire_message(&wire).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_wire_message_group_update_missing_doc_id() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted = crate::crypto::encrypt(&group_key, b"data").unwrap();

        let wire = GossipWireMessage {
            kind: "group_update".to_string(),
            payload: b64.encode(&encrypted),
            doc_id: None, // Missing!
            group_key_id: Some(group_id),
        };

        let result = orch.decode_wire_message(&wire).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_decode_wire_message_sync_missing_doc_id() {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;

        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted_diff = crate::crypto::encrypt(&group_key, b"diff").unwrap();

        let wire = GossipWireMessage {
            kind: "sync_response".to_string(),
            payload: b64.encode(&encrypted_diff),
            doc_id: None, // Missing!
            group_key_id: Some(group_id),
        };

        let result = orch.decode_wire_message(&wire).await;
        assert!(result.is_err());
    }
}
