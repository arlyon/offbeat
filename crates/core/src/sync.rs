//! SyncOrchestrator — unified sync coordination for all resources.
//!
//! This module provides:
//! - `PeerConnection` trait — abstract interface for WS relay and iroh-gossip
//! - `SyncOrchestrator` — coordinates sync for all registered resources
//! - `SyncReport` — summary of sync operations performed

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, RwLock};

use crate::chat::ChatManager;
use crate::connection_manager::ConnectionManager;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::gossip_manager::{DispatchResult, GossipManager, GossipMessage, dispatch_message};
use crate::key_cache::GroupKeyCache;
use crate::notifier::ResourceNotifier;
use crate::proto;
use crate::resource::{Priority, Resource, ResourceKind, ResourceRegistry};
use crate::transport::profile::TransportProfile;
use crate::types::{ChatMessage, SignedUpdate};

// ---------------------------------------------------------------------------
// ChatStateVector — per-writer high water marks for chat sync
// ---------------------------------------------------------------------------

/// A writer's highest observed sequence and the message at that sequence.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ChatWriterHead {
    pub sequence: u64,
    pub message_id: String,
}

/// Per-writer heads for append-log catch-up.
#[derive(Debug, Clone, Default)]
pub struct ChatStateVector {
    pub writers: HashMap<String, ChatWriterHead>,
}

fn group_id_from_state_doc(doc_id: &str) -> Option<&str> {
    let group_id = doc_id.strip_prefix("group/")?.strip_suffix("/state")?;
    (!group_id.is_empty() && !group_id.contains('/')).then_some(group_id)
}

const GROUP_CHAT_CATCHUP_PROOF_PREFIX: &str = "offbeat/group-chat-catchup/v1/";

fn group_id_from_chat_topic(topic: &str) -> Option<&str> {
    let group_id = topic.strip_prefix("group/")?.strip_suffix("/chat")?;
    (!group_id.is_empty() && !group_id.contains('/')).then_some(group_id)
}

pub(crate) fn group_chat_catchup_proof(topic: &str) -> Vec<u8> {
    format!("{GROUP_CHAT_CATCHUP_PROOF_PREFIX}{topic}").into_bytes()
}

impl ChatStateVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build highest contiguous per-writer heads. Equal-sequence variants choose
    /// a deterministic ID so complete histories advertise the same commitment.
    pub fn from_messages(messages: &[ChatMessage]) -> Self {
        let mut sequences: HashMap<String, std::collections::BTreeMap<u64, (String, u64)>> =
            HashMap::new();
        for message in messages {
            sequences
                .entry(message.writer_id())
                .or_default()
                .entry(message.writer_seq)
                .and_modify(|(id, logical_time)| {
                    if message.id.as_str() < id.as_str() {
                        id.clone_from(&message.id);
                        *logical_time = message.logical_time;
                    } else if message.id == *id {
                        *logical_time = (*logical_time).max(message.logical_time);
                    }
                })
                .or_insert_with(|| (message.id.clone(), message.logical_time));
        }

        let writers = sequences
            .into_iter()
            .map(|(writer, messages)| {
                let mut sequence = 0;
                let mut message_id = messages
                    .get(&0)
                    .map(|(id, logical_time)| crate::db::chat_head_commitment(id, *logical_time))
                    .unwrap_or_default();
                while let Some((id, logical_time)) = messages.get(&(sequence + 1)) {
                    sequence += 1;
                    message_id = crate::db::chat_head_commitment(id, *logical_time);
                }
                (
                    writer,
                    ChatWriterHead {
                        sequence,
                        message_id,
                    },
                )
            })
            .collect();
        Self { writers }
    }

    pub fn from_heads(heads: Vec<(String, u64, String)>) -> Self {
        Self {
            writers: heads
                .into_iter()
                .map(|(writer, sequence, message_id)| {
                    (
                        writer,
                        ChatWriterHead {
                            sequence,
                            message_id,
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn sequences(&self) -> HashMap<String, u64> {
        self.writers
            .iter()
            .map(|(writer, head)| (writer.clone(), head.sequence))
            .collect()
    }

    pub fn head_ids(&self) -> HashMap<String, String> {
        self.writers
            .iter()
            .map(|(writer, head)| (writer.clone(), head.message_id.clone()))
            .collect()
    }

    /// Encode as JSON for direct transports.
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(&self.writers).unwrap_or_default()
    }

    /// Decode current head objects or the legacy `{writer: sequence}` map.
    pub fn decode(bytes: &[u8]) -> anyhow::Result<Self> {
        if let Ok(writers) = serde_json::from_slice::<HashMap<String, ChatWriterHead>>(bytes) {
            return Ok(Self { writers });
        }
        let legacy: HashMap<String, u64> = serde_json::from_slice(bytes)?;
        Ok(Self {
            writers: legacy
                .into_iter()
                .map(|(writer, sequence)| {
                    (
                        writer,
                        ChatWriterHead {
                            sequence,
                            message_id: String::new(),
                        },
                    )
                })
                .collect(),
        })
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
pub trait PeerConnection: Send + Sync {
    /// Bandwidth/cost profile for this path. Implementations override this for
    /// BLE or Meshtastic; full paths keep the default.
    fn transport_profile(&self) -> TransportProfile {
        TransportProfile::Full
    }

    /// Subscribe to a set of topic strings.
    fn subscribe(&self, topics: Vec<String>) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Perform state vector exchange for a CRDT doc.
    fn sv_exchange(
        &self,
        doc_id: &str,
        sv: &[u8],
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Request chat messages since our state vector. Returns the gossip
    /// envelopes the peer served (newest history we were missing) so the caller
    /// — which holds the group-key cache — can decode and persist them. A peer
    /// whose responses arrive asynchronously (e.g. the WS relay) returns an
    /// empty vec and applies them on its own receive loop.
    fn chat_catchup(
        &self,
        topic: &str,
        sv: &ChatStateVector,
        limit: u32,
        encrypted_group_proof: &[u8],
    ) -> impl Future<Output = anyhow::Result<Vec<proto::GossipEnvelope>>> + Send;

    /// Broadcast data on a topic.
    fn broadcast(
        &self,
        topic: &str,
        data: &[u8],
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

// ---------------------------------------------------------------------------
// SyncOrchestrator
// ---------------------------------------------------------------------------

/// Orchestrates sync for all registered resources in priority order.
pub struct SyncOrchestrator {
    registry: Arc<RwLock<ResourceRegistry>>,
    doc_manager: Arc<DocManager>,
    #[allow(dead_code)]
    chat_manager: Arc<ChatManager>,
    db: Arc<Database>,
    notifier: Arc<ResourceNotifier>,
    /// Cached festival public keys for signature verification.
    festival_public_keys: Arc<RwLock<HashMap<String, [u8; 32]>>>,
    /// In-memory group key cache.
    key_cache: Arc<GroupKeyCache>,
    /// Gossip manager for querying per-topic neighbor counts.
    gossip_manager: Option<Arc<tokio::sync::Mutex<GossipManager>>>,
    /// Connection manager for finding verified BLE peers.
    connection_manager: Option<Arc<ConnectionManager>>,
    /// Live per-topic gossip neighbor sets, maintained from NeighborUp/Down so
    /// the per-subscription peer count can be reported to the UI without taking
    /// the (frequently-contended) gossip manager lock — and refreshed the moment
    /// membership changes rather than only during a sync.
    topic_neighbors:
        Arc<RwLock<HashMap<iroh_gossip::proto::TopicId, std::collections::HashSet<String>>>>,
}

impl SyncOrchestrator {
    /// Create a new SyncOrchestrator.
    pub fn new(
        registry: Arc<RwLock<ResourceRegistry>>,
        doc_manager: Arc<DocManager>,
        chat_manager: Arc<ChatManager>,
        db: Arc<Database>,
        notifier: Arc<ResourceNotifier>,
    ) -> Self {
        Self {
            registry,
            doc_manager,
            chat_manager,
            db,
            notifier,
            festival_public_keys: Arc::new(RwLock::new(HashMap::new())),
            key_cache: Arc::new(GroupKeyCache::new()),
            gossip_manager: None,
            connection_manager: None,
            topic_neighbors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a gossip membership change for a topic and immediately flush an
    /// updated sync status to watchers (the UI). Called from the gossip event
    /// pump on `NeighborUp` (`up = true`) / `NeighborDown` (`up = false`).
    pub fn set_topic_neighbor(&self, topic: iroh_gossip::proto::TopicId, peer: &str, up: bool) {
        {
            let mut map = match self.topic_neighbors.write() {
                Ok(m) => m,
                Err(_) => return,
            };
            let set = map.entry(topic).or_default();
            if up {
                set.insert(peer.to_string());
            } else {
                set.remove(peer);
            }
        }
        // Flush the refreshed per-topic peer counts to the UI.
        self.notify_sync_status(false);
    }

    /// Set the gossip manager for querying per-topic neighbor counts.
    pub fn set_gossip_manager(&mut self, gm: Arc<tokio::sync::Mutex<GossipManager>>) {
        self.gossip_manager = Some(gm);
    }

    /// Set the connection manager for finding verified BLE peers.
    pub fn set_connection_manager(&mut self, cm: Arc<ConnectionManager>) {
        self.connection_manager = Some(cm);
    }

    /// Spawn a background task that ensures the GossipManager is subscribed
    /// to all topics present in the ResourceRegistry.
    pub fn spawn_subscription_manager(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut known_topics = std::collections::HashSet::new();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;

                let resources = match self.registry.read() {
                    Ok(r) => r
                        .by_priority()
                        .iter()
                        .map(|res| (res.topic(), res.topic_string()))
                        .collect::<Vec<_>>(),
                    Err(_) => continue,
                };

                let gm_arc = match &self.gossip_manager {
                    Some(gm) => gm.clone(),
                    None => continue,
                };

                let mut gm = gm_arc.lock().await;
                for (topic_id, topic_str) in resources {
                    if !known_topics.contains(&topic_id) {
                        // For auto-subscriptions, we don't have a specific festival_id
                        // but we can extract it from the topic string if needed.
                        // For now, we use a placeholder or extract from "group/ID/..."
                        let festival_id = topic_str.split('/').nth(1).unwrap_or("unknown");
                        let is_group = topic_str.starts_with("group/");

                        let lane = if is_group { "group" } else { "public" };
                        tracing::info!(lane, "auto-subscribing to gossip topic");
                        if let Err(e) = gm.subscribe(topic_id, festival_id, is_group, vec![]).await
                        {
                            tracing::warn!(lane, error = %e, "auto-subscription failed");
                        } else {
                            known_topics.insert(topic_id);

                            // PROACTIVE SYNC: Now that we've joined a topic,
                            // immediately join all currently verified BLE peers into it.
                            if let Some(cm) = &self.connection_manager {
                                let verified_peers = cm.list_verified_ble_peers();
                                if !verified_peers.is_empty() {
                                    tracing::info!(
                                        lane,
                                        count = verified_peers.len(),
                                        "syncing new topic with existing BLE peers"
                                    );
                                    let peers: Vec<iroh::EndpointId> = verified_peers
                                        .into_iter()
                                        .filter_map(|p| p.parse().ok())
                                        .collect();
                                    let _ = gm.join_peers_all(peers).await;
                                }
                            }
                        }
                    }
                }
            }
        })
    }

    /// Cache a festival's Ed25519 public key.
    pub fn set_festival_public_key(&self, festival_id: &str, public_key: [u8; 32]) {
        if let Ok(mut map) = self.festival_public_keys.write() {
            map.insert(festival_id.to_string(), public_key);
        }
    }

    /// Verify a festival certificate against the pinned admin root and, only if
    /// it chains to that root, cache the festival key it certifies. Returns
    /// `true` if the cert was trusted and cached. This is the offline-verifiable
    /// path: a cert relayed by a peer is accepted without contacting any server.
    pub fn trust_festival_cert(
        &self,
        cert: &crate::cert::FestivalCert,
        admin_root_pubkey: &[u8; 32],
    ) -> bool {
        match cert.verify(admin_root_pubkey) {
            Some(festival_pubkey) => {
                self.set_festival_public_key(&cert.festival_id, festival_pubkey);
                true
            }
            None => {
                tracing::warn!(
                    festival_id = %cert.festival_id,
                    "rejected festival cert: does not chain to pinned admin root"
                );
                false
            }
        }
    }

    /// Get a festival's public key if cached.
    pub fn get_festival_public_key(&self, festival_id: &str) -> Option<[u8; 32]> {
        self.festival_public_keys
            .read()
            .ok()?
            .get(festival_id)
            .copied()
    }

    /// Pre-populate the group key cache so incoming messages can be decoded
    /// without a DB round-trip.
    pub fn cache_group_key(&self, group_id: &str, key: [u8; 32]) {
        self.key_cache.insert(group_id, key);
    }

    pub fn evict_group_key(&self, group_id: &str) {
        self.key_cache.remove(group_id);
    }

    pub fn hydrate_persisted_groups(&self) -> anyhow::Result<usize> {
        let groups = self.db.load_all_group_keys()?;
        {
            let mut registry = self
                .registry
                .write()
                .map_err(|_| anyhow::anyhow!("resource registry lock poisoned"))?;
            registry.register_groups(&groups);
        }
        for (group_id, key) in &groups {
            self.key_cache.insert(group_id, *key);
        }
        Ok(groups.len())
    }

    pub(crate) fn group_key_for_doc(&self, doc_id: &str) -> anyhow::Result<[u8; 32]> {
        let group_id = group_id_from_state_doc(doc_id)
            .ok_or_else(|| anyhow::anyhow!("invalid group state document ID {doc_id}"))?;
        self.group_key(group_id)
    }

    pub(crate) fn group_key_for_chat_topic(
        &self,
        topic: &str,
    ) -> anyhow::Result<[u8; 32]> {
        let group_id = group_id_from_chat_topic(topic)
            .ok_or_else(|| anyhow::anyhow!("invalid group chat topic {topic}"))?;
        self.group_key(group_id)
    }

    pub(crate) fn encrypted_group_chat_catchup_proof(
        &self,
        topic: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let group_key = self.group_key_for_chat_topic(topic)?;
        crate::crypto::encrypt(&group_key, &group_chat_catchup_proof(topic))
    }

    fn group_key(&self, group_id: &str) -> anyhow::Result<[u8; 32]> {
        self.key_cache
            .get_or_load(group_id, &self.db)?
            .ok_or_else(|| anyhow::anyhow!("unknown group key {group_id}"))
    }

    pub(crate) fn apply_encrypted_group_diff(
        &self,
        doc_id: &str,
        encrypted_diff: &[u8],
    ) -> anyhow::Result<()> {
        let group_key = self.group_key_for_doc(doc_id)?;
        let diff = crate::crypto::decrypt(&group_key, encrypted_diff)?;
        self.doc_manager.apply_update(doc_id, &diff)?;
        self.refresh_group_name(doc_id)?;
        self.notifier.record_received(doc_id);
        self.notifier.notify_doc(doc_id);
        self.notify_sync_status(false);
        Ok(())
    }

    fn refresh_group_name(&self, doc_id: &str) -> anyhow::Result<()> {
        let Some(group_id) = group_id_from_state_doc(doc_id) else {
            return Ok(());
        };
        if let Some(name) = self.doc_manager.read_map_value(doc_id, "name")
            && !name.is_empty()
        {
            self.db.update_group_name(group_id, &name)?;
        }
        Ok(())
    }

    /// Build the resources list from the registry for sync status notifications.
    fn build_resource_statuses(&self) -> Vec<crate::notifier::ResourceSyncStatus> {
        let Ok(reg) = self.registry.read() else {
            return vec![];
        };
        // Per-topic neighbor counts come from our own NeighborUp/Down-maintained
        // map, NOT a try_lock on the gossip manager (which is usually held during
        // sync, silently yielding 0 for every row).
        let neighbors = self.topic_neighbors.read().ok();
        reg.by_priority()
            .iter()
            .map(|r| {
                let id = r.id();
                let (received, sent) = self.notifier.get_counters(&id);
                let peer_count = neighbors
                    .as_ref()
                    .and_then(|n| n.get(&r.topic()))
                    .map(|s| s.len() as u32)
                    .unwrap_or(0);
                crate::notifier::ResourceSyncStatus {
                    id,
                    syncing: false,
                    last_synced: None,
                    error: None,
                    messages_received: received,
                    messages_sent: sent,
                    peer_count,
                }
            })
            .collect()
    }

    /// Run subscribe→catch-up→live for all resources with a peer.
    pub async fn sync_with_peer<P: PeerConnection>(&self, peer: &P) -> anyhow::Result<SyncReport> {
        self.notify_sync_status(true);
        let result = self.sync_with_peer_inner(peer, None).await;
        self.notify_sync_status(false);
        result
    }

    /// Sync only resources belonging to one festival. Relay connections use
    /// this boundary so a Festival DO never learns topics from another event.
    pub async fn sync_with_peer_for_festival<P: PeerConnection>(
        &self,
        peer: &P,
        festival_id: &str,
    ) -> anyhow::Result<SyncReport> {
        self.notify_sync_status(true);
        let result = self.sync_with_peer_inner(peer, Some(festival_id)).await;
        self.notify_sync_status(false);
        result
    }

    /// Emit a sync status update with the current resource list.
    fn notify_sync_status(&self, syncing: bool) {
        let resources = self.build_resource_statuses();
        self.notifier
            .notify_sync_status(crate::notifier::SyncStatus {
                syncing,
                resources,
                pending_ops: 0,
            });
    }

    async fn sync_with_peer_inner<P: PeerConnection>(
        &self,
        peer: &P,
        festival_id: Option<&str>,
    ) -> anyhow::Result<SyncReport> {
        let mut report = SyncReport::default();

        let resources: Vec<(String, ResourceKind, String, Priority)> = {
            let reg = self
                .registry
                .read()
                .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
            let mut resources = Vec::new();
            for resource in reg.by_priority() {
                let matches_festival = match (festival_id, resource) {
                    (None, _) => true,
                    (Some(expected), Resource::FestivalState { festival_id, .. })
                    | (Some(expected), Resource::StageChat { festival_id, .. }) => {
                        festival_id == expected
                    }
                    (Some(expected), Resource::GroupState { group_id, .. })
                    | (Some(expected), Resource::GroupChat { group_id, .. }) => self
                        .db
                        .load_group_festival_id(group_id)?
                        .is_some_and(|festival_id| festival_id == expected),
                };
                if matches_festival {
                    resources.push((
                        resource.id(),
                        resource.kind(),
                        resource.topic_string(),
                        resource.priority(),
                    ));
                }
            }
            resources
        };

        let topics: Vec<String> = resources.iter().map(|(_, _, t, _)| t.clone()).collect();
        if !topics.is_empty() {
            peer.subscribe(topics).await?;
        }

        for (id, kind, topic, _priority) in resources {
            match self.sync_resource_impl(&id, kind, &topic, peer).await {
                Ok((crdt_count, chat_count)) => {
                    report.resources_synced += 1;
                    report.crdt_updates_applied += crdt_count;
                    report.chat_messages_received += chat_count;
                }
                Err(_e) => {
                    tracing::warn!("one resource failed to sync");
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
            let reg = self
                .registry
                .read()
                .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
            let r = reg
                .get(resource_id)
                .ok_or_else(|| anyhow::anyhow!("resource not found: {resource_id}"))?;
            (r.kind(), r.topic_string())
        };

        self.sync_resource_impl(resource_id, kind, &topic, peer)
            .await
            .map(|_| ())
    }

    async fn sync_resource_impl<P: PeerConnection>(
        &self,
        resource_id: &str,
        kind: ResourceKind,
        topic: &str,
        peer: &P,
    ) -> anyhow::Result<(u32, u32)> {
        match kind {
            ResourceKind::CrdtDoc => {
                let sv = {
                    self.doc_manager.get_or_create(resource_id);
                    self.doc_manager.get_state_vector(resource_id)?
                };
                peer.sv_exchange(resource_id, &sv).await?;
                Ok((1, 0))
            }
            ResourceKind::AppendLog => {
                let profile = peer.transport_profile();
                if !profile.allows_chat_catchup() {
                    tracing::debug!(
                        topic,
                        ?profile,
                        "skipping append-log catch-up on constrained transport profile"
                    );
                    return Ok((0, 0));
                }

                let csv = ChatStateVector::from_heads(self.db.get_chat_writer_heads(topic)?);
                let encrypted_group_proof = if topic.starts_with("group/") {
                    self.encrypted_group_chat_catchup_proof(topic)?
                } else {
                    Vec::new()
                };
                let envelopes = peer
                    .chat_catchup(
                        topic,
                        &csv,
                        profile.chat_catchup_limit(),
                        &encrypted_group_proof,
                    )
                    .await?;
                // Validate and persist the complete page against one clock floor
                // so a malicious peer cannot ratchet Lamport time per envelope.
                self.handle_incoming_chat_batch(topic, &envelopes)?;
                Ok((0, envelopes.len() as u32))
            }
        }
    }

    /// Handle an incoming gossip envelope from the wire (protobuf bytes).
    pub async fn handle_incoming_bytes(&self, topic: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let envelope = proto::decode_envelope(bytes)?;
        self.handle_incoming_envelope(topic, &envelope).await
    }

    /// Handle an incoming GossipEnvelope, routing to the correct handler.
    pub fn chat_state_vector(&self, topic: &str) -> anyhow::Result<ChatStateVector> {
        Ok(ChatStateVector::from_heads(
            self.db.get_chat_writer_heads(topic)?,
        ))
    }

    pub fn handle_incoming_chat_batch(
        &self,
        topic: &str,
        envelopes: &[proto::GossipEnvelope],
    ) -> anyhow::Result<()> {
        for envelope in envelopes {
            if let Some(proto::gossip_envelope::Payload::ChatAuthorProof(proof)) = &envelope.payload
            {
                let updated_topics = self.db.save_chat_author_proof(
                    &proof.writer_key,
                    &proof.attestation_message,
                    &proof.attestation_signature,
                    &proof.issuer,
                )?;
                for updated_topic in updated_topics {
                    self.notifier.notify_chat(&updated_topic);
                }
            }
        }

        let mut messages = Vec::with_capacity(envelopes.len());
        for envelope in envelopes {
            let message = match self.decode_envelope(envelope)? {
                Some(GossipMessage::Chat(message)) => message,
                Some(GossipMessage::EncryptedChat {
                    group_key,
                    encrypted,
                }) => {
                    let plaintext = crate::crypto::decrypt(&group_key, &encrypted)?;
                    serde_json::from_slice(&plaintext)
                        .map_err(|error| anyhow::anyhow!("deserialise chat: {error}"))?
                }
                Some(GossipMessage::ChatAuthorProof { .. }) => continue,
                _ => anyhow::bail!("chat catch-up contained a non-chat envelope"),
            };
            if message.topic != topic {
                anyhow::bail!("chat topic mismatch");
            }
            if topic.starts_with("festival/")
                && !crate::signing::verify_public_chat_message(&message)
            {
                anyhow::bail!("invalid public chat signature");
            }
            if message.writer_seq == 0 {
                anyhow::bail!("chat message is missing a writer sequence");
            }
            messages.push(message);
        }
        self.db.save_chat_messages_batch(&messages)?;
        for _ in &messages {
            self.notifier.record_received(topic);
        }
        self.notifier.notify_chat(topic);
        self.notify_sync_status(false);
        Ok(())
    }

    pub async fn handle_incoming_envelope(
        &self,
        topic: &str,
        envelope: &proto::GossipEnvelope,
    ) -> anyhow::Result<()> {
        if topic.starts_with("group/") {
            use proto::gossip_envelope::Payload;
            let routed_doc = match envelope.payload.as_ref() {
                Some(Payload::GroupUpdate(update)) => Some(update.doc_id.as_str()),
                Some(Payload::SyncRequest(request)) => Some(request.doc_id.as_str()),
                Some(Payload::SyncResponse(response)) => Some(response.doc_id.as_str()),
                Some(Payload::SyncUpdate(update)) => Some(update.doc_id.as_str()),
                _ => None,
            };
            if let Some(doc_id) = routed_doc
                && doc_id != topic
            {
                anyhow::bail!("group topic/document mismatch: {topic} != {doc_id}");
            }
        }

        let gossip_msg = self.decode_envelope(envelope)?;
        let Some(ref msg) = gossip_msg else {
            return Ok(());
        };

        let festival_pk = match msg {
            GossipMessage::FestivalUpdate { doc_id, .. } => {
                if !doc_id.starts_with("festival/") || !doc_id.ends_with("/state") {
                    anyhow::bail!("invalid festival document ID {doc_id}");
                }
                if topic.starts_with("festival/") && topic != doc_id {
                    anyhow::bail!("festival topic/document mismatch: {topic} != {doc_id}");
                }
                self.extract_festival_public_key(doc_id)
            }
            GossipMessage::Chat(message) => {
                if message.topic != topic {
                    anyhow::bail!("chat topic mismatch");
                }
                None
            }
            GossipMessage::ChatAuthorProof { .. } => {
                if !topic.starts_with("festival/") || !topic.contains("/chat/") {
                    anyhow::bail!("chat proof routed on a non-public topic");
                }
                None
            }
            _ => None,
        };
        let pk = festival_pk.unwrap_or([0u8; 32]);
        let result = dispatch_message(&self.doc_manager, &self.db, msg.clone(), &pk)?;

        // Notify watchers and record counters after successful dispatch
        match &msg {
            GossipMessage::FestivalUpdate { doc_id, .. } => {
                self.notifier.record_received(doc_id);
                self.notifier.notify_doc(doc_id);
            }
            GossipMessage::GroupUpdate { doc_id, .. }
            | GossipMessage::SyncResponse { doc_id, .. }
            | GossipMessage::SyncUpdate { doc_id, .. } => {
                self.refresh_group_name(doc_id)?;
                self.notifier.record_received(doc_id);
                self.notifier.notify_doc(doc_id);
            }
            GossipMessage::Chat(chat_msg) => {
                self.notifier.record_received(&chat_msg.topic);
                self.notifier.notify_chat(&chat_msg.topic);
            }
            GossipMessage::EncryptedChat { .. } => {
                if let DispatchResult::DecryptedChat { topic } = &result {
                    self.notifier.record_received(topic);
                    self.notifier.notify_chat(topic);
                }
            }
            GossipMessage::ChatAuthorProof { .. } => {
                if let DispatchResult::ChatProofUpdated { topics } = &result {
                    for topic in topics {
                        self.notifier.notify_chat(topic);
                    }
                }
            }
            GossipMessage::SyncRequest { .. } => {}
        }

        // Re-emit sync status so the UI picks up updated counters
        self.notify_sync_status(false);

        Ok(())
    }

    /// Decode a GossipEnvelope using the in-memory key cache (no DB blocking).
    fn decode_envelope(
        &self,
        envelope: &proto::GossipEnvelope,
    ) -> anyhow::Result<Option<GossipMessage>> {
        use proto::gossip_envelope::Payload;

        let payload = envelope
            .payload
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("empty gossip envelope"))?;

        match payload {
            Payload::FestivalUpdate(fu) => {
                let signed = fu
                    .signed_update
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("festival_update missing signed_update"))?;
                Ok(Some(GossipMessage::FestivalUpdate {
                    doc_id: fu.doc_id.clone(),
                    kind: fu.kind,
                    authority_seq: fu.authority_seq,
                    signed_update: SignedUpdate {
                        update: signed.update.clone(),
                        author: signed.author.clone(),
                        signature: signed.signature.clone(),
                    },
                }))
            }

            Payload::Chat(chat) => Ok(Some(GossipMessage::Chat(chat.clone().into()))),

            Payload::ChatAuthorProof(proof) => Ok(Some(GossipMessage::ChatAuthorProof {
                writer_key: proof.writer_key.clone(),
                attestation_message: proof.attestation_message.clone(),
                attestation_signature: proof.attestation_signature.clone(),
                issuer: proof.issuer.clone(),
            })),

            Payload::GroupUpdate(gu) => {
                let group_id = group_id_from_state_doc(&gu.doc_id).ok_or_else(|| {
                    anyhow::anyhow!("group_update: invalid document ID {}", gu.doc_id)
                })?;
                if group_id != gu.group_key_id {
                    anyhow::bail!(
                        "group_update: document/key mismatch {} != {}",
                        group_id,
                        gu.group_key_id
                    );
                }
                let group_key = self
                    .key_cache
                    .get_or_load(&gu.group_key_id, &self.db)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("group_update: unknown group key {}", gu.group_key_id)
                    })?;
                Ok(Some(GossipMessage::GroupUpdate {
                    doc_id: gu.doc_id.clone(),
                    encrypted: gu.encrypted.clone(),
                    group_key,
                }))
            }

            Payload::EncryptedChat(ec) => {
                let group_key = self
                    .key_cache
                    .get_or_load(&ec.group_key_id, &self.db)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("encrypted_chat: unknown group key {}", ec.group_key_id)
                    })?;
                Ok(Some(GossipMessage::EncryptedChat {
                    group_key,
                    encrypted: ec.encrypted.clone(),
                }))
            }

            Payload::SyncRequest(_) => {
                tracing::debug!("sync_request received; handled at higher level");
                Ok(None)
            }

            Payload::SyncResponse(sr) => {
                let group_id = group_id_from_state_doc(&sr.doc_id).ok_or_else(|| {
                    anyhow::anyhow!("sync_response: invalid document ID {}", sr.doc_id)
                })?;
                if group_id != sr.group_key_id {
                    anyhow::bail!(
                        "sync_response: document/key mismatch {} != {}",
                        group_id,
                        sr.group_key_id
                    );
                }
                let group_key = self
                    .key_cache
                    .get_or_load(&sr.group_key_id, &self.db)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("sync_response: unknown group key {}", sr.group_key_id)
                    })?;
                Ok(Some(GossipMessage::SyncResponse {
                    doc_id: sr.doc_id.clone(),
                    encrypted_diff: sr.encrypted_diff.clone(),
                    group_key,
                }))
            }

            Payload::SyncUpdate(su) => {
                let group_id = group_id_from_state_doc(&su.doc_id).ok_or_else(|| {
                    anyhow::anyhow!("sync_update: invalid document ID {}", su.doc_id)
                })?;
                if group_id != su.group_key_id {
                    anyhow::bail!(
                        "sync_update: document/key mismatch {} != {}",
                        group_id,
                        su.group_key_id
                    );
                }
                let group_key = self
                    .key_cache
                    .get_or_load(&su.group_key_id, &self.db)?
                    .ok_or_else(|| {
                        anyhow::anyhow!("sync_update: unknown group key {}", su.group_key_id)
                    })?;
                Ok(Some(GossipMessage::SyncUpdate {
                    doc_id: su.doc_id.clone(),
                    encrypted_diff: su.encrypted_diff.clone(),
                    group_key,
                }))
            }
        }
    }

    /// Extract festival public key from topic string if applicable.
    fn extract_festival_public_key(&self, topic: &str) -> Option<[u8; 32]> {
        let parts: Vec<&str> = topic.split('/').collect();
        if parts.len() >= 2 && (parts[0] == "festival" || parts[0] == "offbeat") {
            return self.get_festival_public_key(parts[1]);
        }
        None
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
                logical_time: 1,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
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
                logical_time: 2,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
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
                logical_time: 1,
                writer_key: Vec::new(),
                signature: Vec::new(),
                trust: crate::types::ChatTrust::Unverified,
            },
        ];

        let csv = ChatStateVector::from_messages(&messages);
        assert_eq!(
            csv.writers.get("alice"),
            Some(&ChatWriterHead {
                sequence: 2,
                message_id: "m2@2".to_string(),
            })
        );
        assert_eq!(csv.sequences().get("bob"), Some(&1));
        assert_eq!(csv.head_ids().get("bob").map(String::as_str), Some("m3@1"));
    }

    #[test]
    fn test_chat_state_vector_encode_decode() {
        let mut csv = ChatStateVector::new();
        csv.writers.insert(
            "alice".to_string(),
            ChatWriterHead {
                sequence: 5,
                message_id: "a5".to_string(),
            },
        );

        let decoded = ChatStateVector::decode(&csv.encode()).unwrap();
        assert_eq!(decoded.writers, csv.writers);

        let legacy = ChatStateVector::decode(br#"{"bob":3}"#).unwrap();
        assert_eq!(legacy.sequences().get("bob"), Some(&3));
        assert_eq!(legacy.head_ids().get("bob").map(String::as_str), Some(""));
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
        let doc_manager = Arc::new(crate::doc_manager::DocManager::new(db.clone()));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(
            db.clone(),
            doc_manager.clone(),
        ));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        let notifier = Arc::new(ResourceNotifier::new());
        SyncOrchestrator::new(registry, doc_manager, chat_manager, db, notifier)
    }

    #[test]
    fn test_orchestrator_festival_public_key_cache() {
        let orch = create_orchestrator();

        assert!(orch.get_festival_public_key("fest1").is_none());

        let key = [42u8; 32];
        orch.set_festival_public_key("fest1", key);
        assert_eq!(orch.get_festival_public_key("fest1"), Some(key));

        assert!(orch.get_festival_public_key("fest2").is_none());
    }

    #[test]
    fn test_extract_festival_public_key_from_topic() {
        let orch = create_orchestrator();

        let key = [99u8; 32];
        orch.set_festival_public_key("glastonbury", key);

        assert_eq!(
            orch.extract_festival_public_key("festival/glastonbury/state"),
            Some(key)
        );
        assert_eq!(
            orch.extract_festival_public_key("offbeat/glastonbury/chat"),
            Some(key)
        );
        assert!(
            orch.extract_festival_public_key("festival/unknown/state")
                .is_none()
        );
        assert!(
            orch.extract_festival_public_key("group/abc123/state")
                .is_none()
        );
        assert!(orch.extract_festival_public_key("invalid").is_none());
    }

    #[tokio::test]
    async fn test_handle_incoming_festival_update() {
        use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

        let orch = create_orchestrator();
        let signing_key = crate::signing::generate_signing_key();
        let public_key = signing_key.verifying_key().to_bytes();
        let doc_id = "festival/test/state";
        orch.set_festival_public_key("test", public_key);

        let source = Doc::new();
        let map = source.get_or_insert_map("root");
        map.insert(&mut source.transact_mut(), "artist", "Aphex Twin");
        let update = source
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let signature =
            crate::signing::sign_festival_update(&signing_key, doc_id, 2, 3, &update).unwrap();
        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::FestivalUpdate(
                proto::FestivalUpdate {
                    doc_id: doc_id.to_string(),
                    kind: 2,
                    authority_seq: 3,
                    signed_update: Some(proto::SignedUpdate {
                        update,
                        author: "festival-do".to_string(),
                        signature,
                    }),
                },
            )),
        };

        // The iroh-gossip event does not expose the unhashed topic string, so
        // festival authority lookup must use the signed document ID.
        orch.handle_incoming_envelope("gossip", &envelope)
            .await
            .unwrap();
        assert_eq!(
            orch.doc_manager.read_map_value(doc_id, "artist").as_deref(),
            Some("Aphex Twin")
        );
        assert_eq!(orch.db.highest_verified_festival_seq(doc_id).unwrap(), 3);
    }

    #[test]
    fn chat_batch_uses_one_remote_clock_floor() {
        let orch = create_orchestrator();
        let topic = "festival/test/chat/general";
        let envelopes: Vec<_> = [1_000_000, 2_000_000]
            .into_iter()
            .enumerate()
            .map(|(index, logical_time)| proto::GossipEnvelope {
                payload: Some(proto::gossip_envelope::Payload::Chat(proto::ChatMessage {
                    id: format!("m{index}"),
                    user_id: "mallory".to_string(),
                    display_name: "Mallory".to_string(),
                    text: "ratchet".to_string(),
                    topic: topic.to_string(),
                    stage_id: None,
                    timestamp: "display-only".to_string(),
                    writer_seq: index as u64 + 1,
                    logical_time,
                    writer_key: Vec::new(),
                    signature: Vec::new(),
                })),
            })
            .collect();

        assert!(orch.handle_incoming_chat_batch(topic, &envelopes).is_err());
        assert!(orch.db.get_chat_messages(topic, 10, 0).unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_handle_incoming_chat() {
        let orch = create_orchestrator();

        let signing_key = crate::signing::generate_signing_key();
        let topic = "festival/test/chat/campsite";
        let mut message = ChatMessage {
            id: "msg1".to_string(),
            user_id: crate::auth::get_user_id(&signing_key),
            display_name: "User".to_string(),
            text: "hello".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-01-01".to_string(),
            writer_seq: 1,
            logical_time: 1,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        crate::signing::sign_public_chat_message(&signing_key, &mut message).unwrap();
        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::Chat(message.into())),
        };

        assert!(
            orch.handle_incoming_envelope("other", &envelope)
                .await
                .is_err()
        );
        orch.handle_incoming_envelope(topic, &envelope)
            .await
            .unwrap();

        let msg = orch.decode_envelope(&envelope).unwrap();
        assert!(matches!(msg, Some(GossipMessage::Chat(_))));

        if let Some(GossipMessage::Chat(chat)) = msg {
            assert_eq!(chat.id, "msg1");
            assert_eq!(chat.text, "hello");
        }
    }

    #[tokio::test]
    async fn test_handle_incoming_empty_envelope() {
        let orch = create_orchestrator();

        let envelope = proto::GossipEnvelope { payload: None };
        let result = orch.decode_envelope(&envelope);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_incoming_sync_request_skipped() {
        let orch = create_orchestrator();

        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::SyncRequest(
                proto::SyncRequest {
                    doc_id: "doc".to_string(),
                    encrypted_sv: vec![],
                    group_key_id: "key".to_string(),
                },
            )),
        };

        let result = orch.decode_envelope(&envelope).unwrap();
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
            _encrypted_group_proof: &[u8],
        ) -> anyhow::Result<Vec<proto::GossipEnvelope>> {
            Ok(vec![])
        }

        async fn broadcast(&self, _topic: &str, _data: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    /// A `PeerConnection` that performs a *real* state-vector exchange against a
    /// separate "remote" `DocManager` and applies the returned diff locally —
    /// modelling the transient ALPN catch-up fired on `NeighborUp`, with no
    /// network. The `offbeat/sync/1` ALPN impl is the same exchange over a wire.
    struct InMemoryPeer {
        remote: Arc<crate::doc_manager::DocManager>,
        local: Arc<crate::doc_manager::DocManager>,
    }

    struct ConstrainedPeer;

    impl PeerConnection for ConstrainedPeer {
        fn transport_profile(&self) -> TransportProfile {
            TransportProfile::Constrained
        }

        async fn subscribe(&self, _topics: Vec<String>) -> anyhow::Result<()> {
            Ok(())
        }

        async fn sv_exchange(&self, _doc_id: &str, _sv: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }

        async fn chat_catchup(
            &self,
            _topic: &str,
            _sv: &ChatStateVector,
            _limit: u32,
            _encrypted_group_proof: &[u8],
        ) -> anyhow::Result<Vec<proto::GossipEnvelope>> {
            panic!("constrained transports must not perform append-log catch-up")
        }

        async fn broadcast(&self, _topic: &str, _data: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    impl PeerConnection for InMemoryPeer {
        async fn subscribe(&self, _topics: Vec<String>) -> anyhow::Result<()> {
            Ok(())
        }

        async fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> anyhow::Result<()> {
            // Remote computes the diff since our state vector; we apply it
            // locally, exactly as the ALPN response would be dispatched.
            let diff = self.remote.encode_diff(doc_id, sv)?;
            self.local.apply_update(doc_id, &diff)?;
            Ok(())
        }

        async fn chat_catchup(
            &self,
            _topic: &str,
            _sv: &ChatStateVector,
            _limit: u32,
            _encrypted_group_proof: &[u8],
        ) -> anyhow::Result<Vec<proto::GossipEnvelope>> {
            Ok(vec![])
        }

        async fn broadcast(&self, _topic: &str, _data: &[u8]) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_constrained_profile_skips_append_log_catchup() {
        let orchestrator = create_orchestrator();
        let report = orchestrator
            .sync_resource_impl(
                "group/test/chat",
                ResourceKind::AppendLog,
                "group/test/chat",
                &ConstrainedPeer,
            )
            .await
            .unwrap();
        assert_eq!(report, (0, 0));
    }

    #[tokio::test]
    async fn test_sv_exchange_converges_stale_local_to_remote() {
        use crate::resource::Resource;
        use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

        let doc_id = "festival/fest1/state";

        // Remote node already has the festival doc populated.
        let remote_db = test_db();
        let remote = Arc::new(crate::doc_manager::DocManager::new(remote_db));
        remote.get_or_create(doc_id);
        let update = {
            let d = Doc::new();
            let m = d.get_or_insert_map("root");
            {
                let mut txn = d.transact_mut();
                m.insert(&mut txn, "stage", "main");
            }
            d.transact()
                .encode_state_as_update_v1(&StateVector::default())
        };
        remote.apply_update(doc_id, &update).unwrap();

        // Local node starts empty, with the festival resource registered.
        let db = test_db();
        let local = Arc::new(crate::doc_manager::DocManager::new(db.clone()));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(db.clone(), local.clone()));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        {
            let mut reg = registry.write().unwrap();
            reg.register(Resource::festival_state("fest1", [0u8; 32]));
        }
        let notifier = Arc::new(ResourceNotifier::new());
        let orch = SyncOrchestrator::new(registry, local.clone(), chat_manager, db, notifier);

        assert_eq!(local.read_map_value(doc_id, "stage"), None);

        // NeighborUp → transient sv_exchange against the neighbor.
        let peer = InMemoryPeer {
            remote: remote.clone(),
            local: local.clone(),
        };
        orch.sync_resource(doc_id, &peer).await.unwrap();

        // Local has converged to the remote's state.
        assert_eq!(
            local.read_map_value(doc_id, "stage"),
            Some("main".to_string())
        );
    }

    #[test]
    fn test_trust_festival_cert_gates_caching_on_pinned_root() {
        use crate::cert::issue_festival_cert;
        use crate::signing;

        let orch = create_orchestrator();
        let admin_root = signing::generate_signing_key();
        let admin_root_pk = admin_root.verifying_key().to_bytes();
        let festival_pk = signing::generate_signing_key().verifying_key().to_bytes();

        // A cert from the pinned root is trusted and cached.
        let cert = issue_festival_cert(&admin_root, "fest1", &festival_pk);
        assert!(orch.trust_festival_cert(&cert, &admin_root_pk));
        assert_eq!(orch.get_festival_public_key("fest1"), Some(festival_pk));

        // A cert from an unpinned (attacker) root is rejected and not cached.
        let attacker = signing::generate_signing_key();
        let bad = issue_festival_cert(&attacker, "fest2", &festival_pk);
        assert!(!orch.trust_festival_cert(&bad, &admin_root_pk));
        assert!(orch.get_festival_public_key("fest2").is_none());
    }

    #[tokio::test]
    async fn test_sync_with_peer_empty_registry() {
        let orch = create_orchestrator();
        let peer = MockPeer::new();

        let report = orch.sync_with_peer(&peer).await.unwrap();

        assert_eq!(report.resources_synced, 0);
        assert!(report.failed.is_empty());
        assert!(peer.subscribed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_sync_with_peer_with_resources() {
        use crate::resource::Resource;

        let db = test_db();
        let doc_manager = Arc::new(crate::doc_manager::DocManager::new(db.clone()));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(
            db.clone(),
            doc_manager.clone(),
        ));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));

        let dummy_key = [0u8; 32];

        {
            let mut reg = registry.write().unwrap();
            reg.register(Resource::festival_state("fest1", dummy_key));
            reg.register(Resource::stage_chat("fest1", "main-stage", dummy_key));
        }

        let notifier = Arc::new(ResourceNotifier::new());
        let orch = SyncOrchestrator::new(registry.clone(), doc_manager, chat_manager, db, notifier);
        let peer = MockPeer::new();

        let report = orch.sync_with_peer(&peer).await.unwrap();

        assert_eq!(report.resources_synced, 2);
        assert!(report.failed.is_empty());

        let subs = peer.subscribed.lock().unwrap();
        assert_eq!(subs.len(), 2);

        let exchanges = peer.sv_exchanges.lock().unwrap();
        assert_eq!(exchanges.len(), 1);
        assert!(exchanges[0].contains("fest1"));
    }

    #[tokio::test]
    async fn relay_sync_is_scoped_to_one_festival() {
        let db = test_db();
        let group_one_key = [1u8; 32];
        let group_two_key = [2u8; 32];
        let group_one_id = crate::crypto::group_id_from_key(&group_one_key);
        let group_two_id = crate::crypto::group_id_from_key(&group_two_key);
        db.save_group(&group_one_id, "fest1", "One", &group_one_key)
            .unwrap();
        db.save_group(&group_two_id, "fest2", "Two", &group_two_key)
            .unwrap();
        let doc_manager = Arc::new(crate::doc_manager::DocManager::new(db.clone()));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(
            db.clone(),
            doc_manager.clone(),
        ));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        {
            let mut reg = registry.write().unwrap();
            reg.register(Resource::festival_state("fest1", [3; 32]));
            reg.register(Resource::festival_state("fest2", [4; 32]));
            reg.register_groups(&[
                (group_one_id.clone(), group_one_key),
                (group_two_id, group_two_key),
            ]);
        }
        let notifier = Arc::new(ResourceNotifier::new());
        let orchestrator = SyncOrchestrator::new(registry, doc_manager, chat_manager, db, notifier);
        let peer = MockPeer::new();

        let report = orchestrator
            .sync_with_peer_for_festival(&peer, "fest1")
            .await
            .unwrap();

        assert_eq!(report.resources_synced, 3);
        let subscriptions = peer.subscribed.lock().unwrap();
        assert_eq!(subscriptions.len(), 3);
        assert!(subscriptions.iter().all(|topic| !topic.contains("fest2")));
        assert!(
            subscriptions
                .iter()
                .all(|topic| !topic.contains(&crate::crypto::group_id_from_key(&group_two_key)))
        );
        assert!(
            subscriptions
                .iter()
                .any(|topic| topic == &format!("group/{group_one_id}/state"))
        );
    }

    #[tokio::test]
    async fn test_sync_resource_not_found() {
        let orch = create_orchestrator();
        let peer = MockPeer::new();

        let result = orch.sync_resource("nonexistent", &peer).await;
        assert!(result.is_err());
    }

    fn create_orchestrator_with_group_key(group_id: &str, group_key: [u8; 32]) -> SyncOrchestrator {
        let db = test_db();
        db.save_group(group_id, "test-fest", "Test Group", &group_key)
            .unwrap();
        let doc_manager = Arc::new(crate::doc_manager::DocManager::new(db.clone()));
        let chat_manager = Arc::new(crate::chat::ChatManager::new(
            db.clone(),
            doc_manager.clone(),
        ));
        let registry = Arc::new(RwLock::new(ResourceRegistry::new()));
        let notifier = Arc::new(ResourceNotifier::new());
        SyncOrchestrator::new(registry, doc_manager, chat_manager, db, notifier)
    }

    #[tokio::test]
    async fn test_decode_group_update_with_valid_key() {
        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted = crate::crypto::encrypt(&group_key, b"test data").unwrap();

        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::GroupUpdate(
                proto::GroupUpdate {
                    doc_id: format!("group/{group_id}/state"),
                    encrypted: encrypted.clone(),
                    group_key_id: group_id.clone(),
                },
            )),
        };

        let result = orch.decode_envelope(&envelope).unwrap();
        assert!(matches!(result, Some(GossipMessage::GroupUpdate { .. })));

        if let Some(GossipMessage::GroupUpdate {
            doc_id,
            encrypted: enc,
            group_key: key,
        }) = result
        {
            assert_eq!(doc_id, format!("group/{group_id}/state"));
            assert_eq!(key, group_key);
            assert_eq!(enc, encrypted);
        }
    }

    #[tokio::test]
    async fn test_decode_encrypted_chat_with_valid_key() {
        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted = crate::crypto::encrypt(&group_key, b"chat message").unwrap();

        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::EncryptedChat(
                proto::EncryptedPayload {
                    encrypted: encrypted.clone(),
                    group_key_id: group_id.clone(),
                },
            )),
        };

        let result = orch.decode_envelope(&envelope).unwrap();
        assert!(matches!(result, Some(GossipMessage::EncryptedChat { .. })));
    }

    #[tokio::test]
    async fn test_decode_sync_response_with_valid_key() {
        let group_key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&group_key);
        let orch = create_orchestrator_with_group_key(&group_id, group_key);

        let encrypted_diff = crate::crypto::encrypt(&group_key, b"diff data").unwrap();

        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::SyncResponse(
                proto::SyncResponse {
                    doc_id: format!("group/{group_id}/state"),
                    encrypted_diff: encrypted_diff.clone(),
                    group_key_id: group_id.clone(),
                },
            )),
        };

        let result = orch.decode_envelope(&envelope).unwrap();
        assert!(matches!(result, Some(GossipMessage::SyncResponse { .. })));
    }
}
