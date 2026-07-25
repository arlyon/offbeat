use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use iroh::EndpointId;
use iroh_gossip::net::Gossip;
use iroh_gossip::proto::TopicId;
use prost::Message;

use crate::crypto;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::proto;
use crate::types::{ChatMessage, SignedUpdate};

// ---------------------------------------------------------------------------
// GossipMessage — the semantic message type used inside the node
// ---------------------------------------------------------------------------

/// Messages received from the gossip network.
#[derive(Debug, Clone)]
pub enum GossipMessage {
    FestivalUpdate {
        doc_id: String,
        kind: i32,
        authority_seq: u64,
        signed_update: SignedUpdate,
    },
    GroupUpdate {
        doc_id: String,
        encrypted: Vec<u8>,
        group_key: [u8; 32],
    },
    Chat(ChatMessage),
    EncryptedChat {
        group_key: [u8; 32],
        encrypted: Vec<u8>,
    },
    /// Peer is requesting a sync: payload is an encrypted Yrs state vector.
    SyncRequest {
        doc_id: String,
        /// Encrypted Yrs state vector bytes.
        encrypted_sv: Vec<u8>,
        group_key: [u8; 32],
    },
    /// Response to a sync_request: payload is an encrypted Yrs diff.
    SyncResponse {
        doc_id: String,
        /// Encrypted Yrs update (diff since requester's SV).
        encrypted_diff: Vec<u8>,
        group_key: [u8; 32],
    },
    /// A single incremental update (diff from one change), encrypted.
    SyncUpdate {
        doc_id: String,
        encrypted_diff: Vec<u8>,
        group_key: [u8; 32],
    },
}

// ---------------------------------------------------------------------------
// dispatch_message
// ---------------------------------------------------------------------------

/// Result of dispatching a gossip message, carrying any info needed for
/// notifications that the caller cannot derive from the original message.
pub enum DispatchResult {
    /// No extra info needed for notification.
    Ok,
    /// An encrypted chat was decrypted; carries the plaintext topic so the
    /// caller can notify chat watchers.
    DecryptedChat { topic: String },
}

/// Dispatch an incoming gossip message to the appropriate handler.
///
/// - `FestivalUpdate` → verifies Ed25519 signature and applies the Yrs update.
/// - `GroupUpdate`    → decrypts with the group key and applies the Yrs update.
/// - `Chat`           → persists to the database.
/// - `EncryptedChat`  → decrypts with the group key, deserialises, and persists.
pub fn dispatch_message(
    doc_manager: &DocManager,
    db: &Database,
    msg: GossipMessage,
    festival_public_key: &[u8; 32],
) -> anyhow::Result<DispatchResult> {
    match msg {
        GossipMessage::FestivalUpdate {
            doc_id,
            kind,
            authority_seq,
            signed_update,
        } => {
            doc_manager.apply_signed_festival_update(
                &doc_id,
                kind,
                authority_seq,
                &signed_update,
                festival_public_key,
            )?;
        }

        GossipMessage::GroupUpdate {
            doc_id,
            encrypted,
            group_key,
        } => {
            doc_manager.apply_encrypted_update(&doc_id, &encrypted, &group_key)?;
        }

        GossipMessage::Chat(msg) => {
            db.save_chat_message(&msg)?;
        }

        GossipMessage::EncryptedChat {
            group_key,
            encrypted,
        } => {
            let plaintext = crypto::decrypt(&group_key, &encrypted)?;
            let chat: ChatMessage = serde_json::from_slice(&plaintext)
                .map_err(|e| anyhow::anyhow!("deserialise chat: {e}"))?;
            let topic = chat.topic.clone();
            db.save_chat_message(&chat)?;
            return Ok(DispatchResult::DecryptedChat { topic });
        }

        GossipMessage::SyncResponse {
            doc_id,
            encrypted_diff,
            group_key,
        } => {
            doc_manager.apply_encrypted_update(&doc_id, &encrypted_diff, &group_key)?;
        }

        GossipMessage::SyncUpdate {
            doc_id,
            encrypted_diff,
            group_key,
        } => {
            doc_manager.apply_encrypted_update(&doc_id, &encrypted_diff, &group_key)?;
        }

        // SyncRequest is handled at a higher level (requires sending a response).
        // If it somehow ends up here, log a warning and skip.
        GossipMessage::SyncRequest { doc_id, .. } => {
            tracing::warn!(
                "dispatch_message: unhandled sync_request for doc {doc_id}; handle at gossip layer"
            );
        }
    }

    Ok(DispatchResult::Ok)
}

// ---------------------------------------------------------------------------
// GossipManager — iroh-gossip networking layer
// ---------------------------------------------------------------------------

/// Receiver handle for gossip events on a topic.
pub type GossipReceiver = iroh_gossip::api::GossipReceiver;

/// Manages iroh-gossip subscriptions and broadcasts.
///
/// This is a simplified version that only handles subscribe/unsubscribe/broadcast.
/// Message dispatch is handled by `SyncOrchestrator`.
pub struct GossipManager {
    gossip: Gossip,
    /// Active topic senders, keyed by TopicId.
    subscriptions: HashMap<TopicId, iroh_gossip::api::GossipSender>,
    /// Active topic receivers, keyed by TopicId.
    receivers: HashMap<TopicId, GossipReceiver>,
    /// Maps each subscribed topic to its metadata (festival + group flag), so
    /// the event pump can scope neighbor harvest and weight dial priority.
    /// Topic IDs are one-way blake3 hashes, so this association can't be
    /// recovered from the topic alone — it's recorded at subscribe time.
    topic_meta: HashMap<TopicId, TopicMeta>,
}

/// Per-topic metadata recorded at subscribe time.
#[derive(Debug, Clone)]
struct TopicMeta {
    festival_id: String,
    /// True for private group topics (higher catch-up priority than public).
    is_group: bool,
}

impl GossipManager {
    pub fn new(gossip: Gossip) -> Self {
        Self {
            gossip,
            subscriptions: HashMap::new(),
            receivers: HashMap::new(),
            topic_meta: HashMap::new(),
        }
    }

    /// Join a gossip topic for `festival_id`, bootstrapping the HyParView
    /// overlay from `bootstrap`. `is_group` marks private group topics, which
    /// get higher catch-up dial priority than public festival topics. The
    /// receiver is stored internally and claimed by the event pump via
    /// [`take_receivers`].
    pub async fn subscribe(
        &mut self,
        topic_id: TopicId,
        festival_id: &str,
        is_group: bool,
        bootstrap: Vec<EndpointId>,
    ) -> anyhow::Result<()> {
        let topic = self.gossip.subscribe(topic_id, bootstrap).await?;
        let (sender, receiver) = topic.split();
        self.subscriptions.insert(topic_id, sender);
        self.receivers.insert(topic_id, receiver);
        self.topic_meta.insert(
            topic_id,
            TopicMeta {
                festival_id: festival_id.to_string(),
                is_group,
            },
        );
        Ok(())
    }

    /// The festival a subscribed topic belongs to, if known.
    pub fn festival_for_topic(&self, topic_id: &TopicId) -> Option<String> {
        self.topic_meta.get(topic_id).map(|m| m.festival_id.clone())
    }

    /// Whether a subscribed topic is a private group topic (higher dial
    /// priority). Defaults to `false` for unknown/public topics.
    pub fn is_group_topic(&self, topic_id: &TopicId) -> bool {
        self.topic_meta.get(topic_id).is_some_and(|m| m.is_group)
    }

    /// Call `join_peers` on every active subscription's sender.
    ///
    /// Used by BLE discovery/reconnect ticks to nudge gossip into dialling
    /// newly-discovered or reconnecting peers across all topics.
    pub async fn join_peers_all(&self, peers: Vec<EndpointId>) {
        for (topic_id, sender) in &self.subscriptions {
            if let Err(e) = sender.join_peers(peers.clone()).await {
                tracing::debug!(?topic_id, "join_peers_all failed for topic: {e}");
            }
        }
    }

    /// Drain all stored receivers so the gossip event pump can claim them.
    ///
    /// Each receiver is returned exactly once; subsequent calls return an
    /// empty map (until new subscriptions are created).
    pub fn take_receivers(&mut self) -> HashMap<TopicId, GossipReceiver> {
        std::mem::take(&mut self.receivers)
    }

    /// Number of active topic subscriptions.
    pub fn topic_count(&self) -> usize {
        self.subscriptions.len()
    }

    /// Leave a previously joined gossip topic.
    pub fn unsubscribe(&mut self, topic_id: TopicId) {
        self.subscriptions.remove(&topic_id);
        self.receivers.remove(&topic_id);
        self.topic_meta.remove(&topic_id);
    }

    /// Get the number of gossip neighbors for a given topic.
    pub fn neighbor_count(&self, topic_id: &TopicId) -> usize {
        self.receivers
            .get(topic_id)
            .map(|r| r.neighbors().count())
            .unwrap_or(0)
    }

    /// Broadcast raw bytes to all peers on the given topic.
    pub async fn broadcast(&mut self, topic_id: TopicId, data: Vec<u8>) -> anyhow::Result<()> {
        let sender = self
            .subscriptions
            .get_mut(&topic_id)
            .ok_or_else(|| anyhow::anyhow!("not subscribed to topic {topic_id:?}"))?;
        sender.broadcast(Bytes::from(data)).await?;
        Ok(())
    }

    /// Encode a `GossipMessage` as protobuf and broadcast it on the given topic.
    pub async fn broadcast_message(
        &mut self,
        topic_id: TopicId,
        msg: &GossipMessage,
    ) -> anyhow::Result<()> {
        let envelope = proto::GossipEnvelope::from_gossip_message(msg);
        let bytes = envelope.encode_to_vec();
        self.broadcast(topic_id, bytes).await
    }

    /// Check if subscribed to a topic.
    pub fn is_subscribed(&self, topic_id: &TopicId) -> bool {
        self.subscriptions.contains_key(topic_id)
    }
}

// ---------------------------------------------------------------------------
// Wire encoding / decoding helpers
// ---------------------------------------------------------------------------

/// Encode a `GossipMessage` into protobuf bytes.
pub fn encode_gossip_message(msg: &GossipMessage) -> Vec<u8> {
    let envelope = proto::GossipEnvelope::from_gossip_message(msg);
    envelope.encode_to_vec()
}

/// Public entry point for `ws_relay` and other callers that receive raw
/// wire bytes and need to dispatch them.
pub async fn handle_wire_bytes_pub(
    raw: &[u8],
    doc_manager: &Arc<DocManager>,
    db: &Arc<Database>,
    festival_public_key: [u8; 32],
) -> anyhow::Result<()> {
    handle_wire_bytes(raw, doc_manager, db, festival_public_key).await
}

/// Decode a `GossipEnvelope` from raw bytes and dispatch it.
async fn handle_wire_bytes(
    raw: &[u8],
    doc_manager: &Arc<DocManager>,
    db: &Arc<Database>,
    festival_public_key: [u8; 32],
) -> anyhow::Result<()> {
    let envelope = proto::decode_envelope(raw)?;
    let gossip_msg = decode_envelope_to_message(&envelope, db).await?;
    let gossip_msg = match gossip_msg {
        Some(m) => m,
        None => return Ok(()),
    };

    dispatch_message(doc_manager, db, gossip_msg, &festival_public_key)?;
    Ok(())
}

/// Decode a GossipEnvelope into a GossipMessage, performing DB key lookups
/// via spawn_blocking. Returns None for messages that should be skipped.
pub async fn decode_envelope_to_message(
    envelope: &proto::GossipEnvelope,
    db: &Arc<Database>,
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

        Payload::GroupUpdate(gu) => {
            let db_clone = Arc::clone(db);
            let key_id = gu.group_key_id.clone();
            let group_key = tokio::task::spawn_blocking(move || db_clone.load_group_key(&key_id))
                .await??
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
            let db_clone = Arc::clone(db);
            let key_id = ec.group_key_id.clone();
            let group_key = tokio::task::spawn_blocking(move || db_clone.load_group_key(&key_id))
                .await??
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
            let db_clone = Arc::clone(db);
            let key_id = sr.group_key_id.clone();
            let group_key = tokio::task::spawn_blocking(move || db_clone.load_group_key(&key_id))
                .await??
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
            let db_clone = Arc::clone(db);
            let key_id = su.group_key_id.clone();
            let group_key = tokio::task::spawn_blocking(move || db_clone.load_group_key(&key_id))
                .await??
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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing;
    use std::sync::Arc;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    #[test]
    fn test_dispatch_chat_stored() {
        let db_arc = test_db();
        let doc_mgr = DocManager::new(db_arc.clone());

        let msg = ChatMessage {
            id: "m1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "hello festival!".to_string(),
            topic: "festival/f1".to_string(),
            stage_id: None,
            timestamp: "2026-06-14T20:00:00Z".to_string(),
            writer_seq: 0,
        };

        let dummy_pk = [0u8; 32];
        dispatch_message(
            &doc_mgr,
            &db_arc,
            GossipMessage::Chat(msg.clone()),
            &dummy_pk,
        )
        .unwrap();

        let stored = db_arc.get_chat_messages("festival/f1", 10, 0).unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text, "hello festival!");
    }

    #[test]
    fn test_dispatch_festival_update_valid_sig() {
        let db_arc = test_db();
        let doc_mgr = DocManager::new(db_arc.clone());

        let signing_key = signing::generate_signing_key();
        let public_key: [u8; 32] = signing_key.verifying_key().to_bytes();

        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "stage", "main");
        }
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());

        let sig =
            signing::sign_festival_update(&signing_key, "fest-doc", 2, 1, &update_bytes).unwrap();
        let signed = SignedUpdate {
            update: update_bytes,
            author: "organiser".to_string(),
            signature: sig,
        };

        dispatch_message(
            &doc_mgr,
            &db_arc,
            GossipMessage::FestivalUpdate {
                doc_id: "fest-doc".to_string(),
                kind: 2,
                authority_seq: 1,
                signed_update: signed,
            },
            &public_key,
        )
        .unwrap();

        let val = doc_mgr.read_map_value("fest-doc", "stage");
        assert_eq!(val, Some("main".to_string()));
    }

    #[test]
    fn test_dispatch_group_update() {
        let db_arc = test_db();
        let doc_mgr = DocManager::new(db_arc.clone());

        let group_key = crypto::generate_group_key();

        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "pin", "tent-area");
        }
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());

        let encrypted = crypto::encrypt(&group_key, &update_bytes).unwrap();

        let dummy_pk = [0u8; 32];
        dispatch_message(
            &doc_mgr,
            &db_arc,
            GossipMessage::GroupUpdate {
                doc_id: "group-doc".to_string(),
                encrypted,
                group_key,
            },
            &dummy_pk,
        )
        .unwrap();

        let val = doc_mgr.read_map_value("group-doc", "pin");
        assert_eq!(val, Some("tent-area".to_string()));
    }

    #[test]
    fn test_protobuf_roundtrip_chat() {
        let chat = ChatMessage {
            id: "w1".to_string(),
            user_id: "u2".to_string(),
            display_name: "Bob".to_string(),
            text: "wire test".to_string(),
            topic: "festival/test/chat".to_string(),
            stage_id: None,
            timestamp: "2026-06-14T21:00:00Z".to_string(),
            writer_seq: 0,
        };
        let bytes = encode_gossip_message(&GossipMessage::Chat(chat.clone()));
        let envelope = proto::decode_envelope(&bytes).unwrap();
        match &envelope.payload {
            Some(proto::gossip_envelope::Payload::Chat(c)) => {
                assert_eq!(c.text, chat.text);
                assert_eq!(c.id, chat.id);
            }
            other => panic!("expected Chat, got {other:?}"),
        }
    }

    #[test]
    fn test_protobuf_roundtrip_festival_update() {
        let signing_key = signing::generate_signing_key();
        let update_bytes = b"fake-yrs-update".to_vec();
        let sig = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: update_bytes,
            author: "organiser".to_string(),
            signature: sig,
        };
        let bytes = encode_gossip_message(&GossipMessage::FestivalUpdate {
            doc_id: "doc-1".to_string(),
            kind: 1,
            authority_seq: 7,
            signed_update: signed.clone(),
        });
        let envelope = proto::decode_envelope(&bytes).unwrap();
        match &envelope.payload {
            Some(proto::gossip_envelope::Payload::FestivalUpdate(fu)) => {
                assert_eq!(fu.doc_id, "doc-1");
                assert_eq!(fu.kind, 1);
                assert_eq!(fu.authority_seq, 7);
                let su = fu.signed_update.as_ref().unwrap();
                assert_eq!(su.author, signed.author);
                assert_eq!(su.update, signed.update);
            }
            other => panic!("expected FestivalUpdate, got {other:?}"),
        }
    }

    #[test]
    fn test_protobuf_roundtrip_sync_request() {
        let group_key = crypto::generate_group_key();
        let fake_sv = b"fake-yrs-sv-bytes";
        let encrypted_sv = crypto::encrypt(&group_key, fake_sv).unwrap();

        let msg = GossipMessage::SyncRequest {
            doc_id: "group/doc-1".to_string(),
            encrypted_sv: encrypted_sv.clone(),
            group_key,
        };
        let bytes = encode_gossip_message(&msg);
        let envelope = proto::decode_envelope(&bytes).unwrap();
        match &envelope.payload {
            Some(proto::gossip_envelope::Payload::SyncRequest(sr)) => {
                assert_eq!(sr.doc_id, "group/doc-1");
                assert_eq!(sr.encrypted_sv, encrypted_sv);
                let plaintext = crypto::decrypt(&group_key, &sr.encrypted_sv).unwrap();
                assert_eq!(plaintext, fake_sv);
            }
            other => panic!("expected SyncRequest, got {other:?}"),
        }
    }

    #[test]
    fn test_protobuf_roundtrip_sync_response() {
        let group_key = crypto::generate_group_key();
        let fake_diff = b"fake-yrs-diff-bytes";
        let encrypted_diff = crypto::encrypt(&group_key, fake_diff).unwrap();

        let msg = GossipMessage::SyncResponse {
            doc_id: "group/doc-2".to_string(),
            encrypted_diff: encrypted_diff.clone(),
            group_key,
        };
        let bytes = encode_gossip_message(&msg);
        let envelope = proto::decode_envelope(&bytes).unwrap();
        match &envelope.payload {
            Some(proto::gossip_envelope::Payload::SyncResponse(sr)) => {
                assert_eq!(sr.doc_id, "group/doc-2");
                let plaintext = crypto::decrypt(&group_key, &sr.encrypted_diff).unwrap();
                assert_eq!(plaintext, fake_diff);
            }
            other => panic!("expected SyncResponse, got {other:?}"),
        }
    }
}
