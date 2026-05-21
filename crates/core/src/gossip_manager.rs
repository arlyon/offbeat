use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use iroh::EndpointId;
use iroh_gossip::net::Gossip;
use iroh_gossip::proto::TopicId;
use tokio::sync::Mutex;

use crate::crypto;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::types::{ChatMessage, SignedUpdate};

// ---------------------------------------------------------------------------
// GossipMessage — the semantic message type used inside the node
// ---------------------------------------------------------------------------

/// Messages received from the gossip network.
#[derive(Debug, Clone)]
pub enum GossipMessage {
    FestivalUpdate {
        doc_id: String,
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
// Wire format — serialised over gossip / WS relay
// ---------------------------------------------------------------------------

/// Flat wire message that travels over iroh-gossip or the WS relay.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GossipWireMessage {
    /// Discriminator: "festival_update" | "group_update" | "chat" | "encrypted_chat"
    ///               | "sync_request" | "sync_response" | "sync_update"
    pub kind: String,
    /// Document ID for CRDT-backed messages.
    pub doc_id: Option<String>,
    /// Base64-encoded binary payload or JSON string.
    pub payload: String,
    /// Base64-encoded group key ID so the receiver can look up the right key.
    pub group_key_id: Option<String>,
}

// ---------------------------------------------------------------------------
// dispatch_message — unchanged from Phase 4
// ---------------------------------------------------------------------------

/// Dispatch an incoming gossip message to the appropriate handler.
///
/// - `FestivalUpdate` → verifies Ed25519 signature and applies the Yrs update.
/// - `GroupUpdate`    → decrypts with the group key and applies the Yrs update.
/// - `Chat`           → persists to the database.
/// - `EncryptedChat`  → decrypts with the group key, deserialises, and persists.
pub fn dispatch_message(
    doc_manager: &mut DocManager,
    db: &Database,
    msg: GossipMessage,
    festival_public_key: &[u8; 32],
) -> anyhow::Result<()> {
    match msg {
        GossipMessage::FestivalUpdate {
            doc_id,
            signed_update,
        } => {
            doc_manager.apply_signed_update(&doc_id, &signed_update, festival_public_key)?;
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
            db.save_chat_message(&chat)?;
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
            tracing::warn!("dispatch_message: unhandled sync_request for doc {doc_id}; handle at gossip layer");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// GossipManager — iroh-gossip networking layer (simplified)
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
}

impl GossipManager {
    pub fn new(gossip: Gossip) -> Self {
        Self {
            gossip,
            subscriptions: HashMap::new(),
            receivers: HashMap::new(),
        }
    }

    /// Join a gossip topic and return a receiver for events.
    ///
    /// `bootstrap` is the list of peer endpoint IDs already on the topic.
    /// The caller is responsible for draining the receiver.
    pub async fn subscribe(
        &mut self,
        topic_id: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> anyhow::Result<GossipReceiver> {
        let topic = self.gossip.subscribe(topic_id, bootstrap).await?;
        let (sender, receiver) = topic.split();
        self.subscriptions.insert(topic_id, sender);
        // Return a clone of the receiver
        Ok(receiver)
    }

    /// Leave a previously joined gossip topic.
    pub fn unsubscribe(&mut self, topic_id: TopicId) {
        // Dropping the sender causes the gossip actor to leave the topic.
        self.subscriptions.remove(&topic_id);
        self.receivers.remove(&topic_id);
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

    /// Encode a `GossipMessage` and broadcast it on the given topic.
    pub async fn broadcast_message(
        &mut self,
        topic_id: TopicId,
        msg: &GossipMessage,
    ) -> anyhow::Result<()> {
        let wire = encode_gossip_message(msg)?;
        let bytes = serde_json::to_vec(&wire)?;
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

/// Encode a `GossipMessage` into the wire format.  Exposed for use by the
/// bridge and other crates.
pub fn encode_gossip_message_pub(msg: &GossipMessage) -> anyhow::Result<GossipWireMessage> {
    encode_gossip_message(msg)
}

fn encode_gossip_message(msg: &GossipMessage) -> anyhow::Result<GossipWireMessage> {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;

    match msg {
        GossipMessage::FestivalUpdate {
            doc_id,
            signed_update,
        } => Ok(GossipWireMessage {
            kind: "festival_update".to_string(),
            doc_id: Some(doc_id.clone()),
            payload: serde_json::to_string(signed_update)?,
            group_key_id: None,
        }),

        GossipMessage::GroupUpdate {
            doc_id,
            encrypted,
            group_key,
        } => Ok(GossipWireMessage {
            kind: "group_update".to_string(),
            doc_id: Some(doc_id.clone()),
            payload: b64.encode(encrypted),
            group_key_id: Some(crypto::group_id_from_key(group_key)),
        }),

        GossipMessage::Chat(chat) => Ok(GossipWireMessage {
            kind: "chat".to_string(),
            doc_id: None,
            payload: serde_json::to_string(chat)?,
            group_key_id: None,
        }),

        GossipMessage::EncryptedChat { group_key, encrypted } => Ok(GossipWireMessage {
            kind: "encrypted_chat".to_string(),
            doc_id: None,
            payload: b64.encode(encrypted),
            group_key_id: Some(crypto::group_id_from_key(group_key)),
        }),

        GossipMessage::SyncRequest {
            doc_id,
            encrypted_sv,
            group_key,
        } => Ok(GossipWireMessage {
            kind: "sync_request".to_string(),
            doc_id: Some(doc_id.clone()),
            payload: b64.encode(encrypted_sv),
            group_key_id: Some(crypto::group_id_from_key(group_key)),
        }),

        GossipMessage::SyncResponse {
            doc_id,
            encrypted_diff,
            group_key,
        } => Ok(GossipWireMessage {
            kind: "sync_response".to_string(),
            doc_id: Some(doc_id.clone()),
            payload: b64.encode(encrypted_diff),
            group_key_id: Some(crypto::group_id_from_key(group_key)),
        }),

        GossipMessage::SyncUpdate {
            doc_id,
            encrypted_diff,
            group_key,
        } => Ok(GossipWireMessage {
            kind: "sync_update".to_string(),
            doc_id: Some(doc_id.clone()),
            payload: b64.encode(encrypted_diff),
            group_key_id: Some(crypto::group_id_from_key(group_key)),
        }),
    }
}

/// Public entry point for `ws_relay` and other callers that receive raw
/// wire bytes and need to dispatch them.
pub async fn handle_wire_bytes_pub(
    raw: &[u8],
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
    festival_public_key: [u8; 32],
) -> anyhow::Result<()> {
    handle_wire_bytes(raw, doc_manager, db, festival_public_key).await
}

/// Decode a `GossipWireMessage` from raw bytes and dispatch it.
///
/// `festival_public_key` is needed only for `festival_update` messages.
///
/// DB calls are wrapped in `spawn_blocking` to avoid blocking the tokio
/// runtime on synchronous SQLite I/O.
async fn handle_wire_bytes(
    raw: &[u8],
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
    festival_public_key: [u8; 32],
) -> anyhow::Result<()> {
    let wire: GossipWireMessage = serde_json::from_slice(raw)
        .map_err(|e| anyhow::anyhow!("deserialise wire message: {e}"))?;

    let gossip_msg = decode_wire_message(wire, db).await?;
    let gossip_msg = match gossip_msg {
        Some(m) => m,
        None => return Ok(()),
    };

    let mut dm = doc_manager.lock().await;
    dispatch_message(&mut dm, db, gossip_msg, &festival_public_key)
}

/// Decode a wire message into a GossipMessage, performing DB key lookups
/// via spawn_blocking. Returns None for messages that should be skipped.
async fn decode_wire_message(
    wire: GossipWireMessage,
    db: &Arc<Database>,
) -> anyhow::Result<Option<GossipMessage>> {
    match wire.kind.as_str() {
        "festival_update" => {
            let signed_update: SignedUpdate = serde_json::from_str(&wire.payload)?;
            Ok(Some(GossipMessage::FestivalUpdate {
                doc_id: wire
                    .doc_id
                    .ok_or_else(|| anyhow::anyhow!("festival_update missing doc_id"))?,
                signed_update,
            }))
        }
        "chat" => {
            let chat: ChatMessage = serde_json::from_str(&wire.payload)?;
            Ok(Some(GossipMessage::Chat(chat)))
        }
        "group_update" => {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD;

            let key_id = wire
                .group_key_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("group_update missing group_key_id"))?
                .to_string();

            let db_clone = Arc::clone(db);
            let key_id_clone = key_id.clone();
            let group_key = tokio::task::spawn_blocking(move || {
                db_clone.load_group_key(&key_id_clone)
            })
            .await??
            .ok_or_else(|| anyhow::anyhow!("group_update: unknown group key {key_id}"))?;

            let encrypted = b64
                .decode(&wire.payload)
                .map_err(|e| anyhow::anyhow!("group_update: base64 decode: {e}"))?;

            Ok(Some(GossipMessage::GroupUpdate {
                doc_id: wire
                    .doc_id
                    .ok_or_else(|| anyhow::anyhow!("group_update missing doc_id"))?,
                encrypted,
                group_key,
            }))
        }

        "encrypted_chat" => {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD;

            let key_id = wire
                .group_key_id
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("encrypted_chat missing group_key_id"))?
                .to_string();

            let db_clone = Arc::clone(db);
            let key_id_clone = key_id.clone();
            let group_key = tokio::task::spawn_blocking(move || {
                db_clone.load_group_key(&key_id_clone)
            })
            .await??
            .ok_or_else(|| anyhow::anyhow!("encrypted_chat: unknown group key {key_id}"))?;

            let encrypted = b64
                .decode(&wire.payload)
                .map_err(|e| anyhow::anyhow!("encrypted_chat: base64 decode: {e}"))?;

            Ok(Some(GossipMessage::EncryptedChat { group_key, encrypted }))
        }

        "sync_request" | "sync_response" | "sync_update" => {
            tracing::warn!(
                "gossip: {} requires group key lookup (key_id={:?}); skipping in generic wire path",
                wire.kind,
                wire.group_key_id
            );
            Ok(None)
        }
        other => {
            tracing::warn!("gossip: unknown message kind: {other}; skipping");
            Ok(None)
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
    use base64::Engine as _;
    use std::sync::Arc;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    #[test]
    fn test_dispatch_chat_stored() {
        let db_arc = test_db();
        let mut doc_mgr = DocManager::new(db_arc.clone());

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
            &mut doc_mgr,
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
        let mut doc_mgr = DocManager::new(db_arc.clone());

        let signing_key = signing::generate_signing_key();
        let public_key: [u8; 32] = signing_key.verifying_key().to_bytes();

        // Build a Yrs update
        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "stage", "main");
        }
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());

        let engine = base64::engine::general_purpose::STANDARD;
        let sig = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: engine.encode(&update_bytes),
            author: "organiser".to_string(),
            signature: engine.encode(&sig),
        };

        dispatch_message(
            &mut doc_mgr,
            &db_arc,
            GossipMessage::FestivalUpdate {
                doc_id: "fest-doc".to_string(),
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
        let mut doc_mgr = DocManager::new(db_arc.clone());

        let group_key = crypto::generate_group_key();

        // Build a Yrs update
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
            &mut doc_mgr,
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
    fn test_wire_roundtrip_chat() {
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
        let wire = encode_gossip_message(&GossipMessage::Chat(chat.clone())).unwrap();
        assert_eq!(wire.kind, "chat");
        let decoded: ChatMessage = serde_json::from_str(&wire.payload).unwrap();
        assert_eq!(decoded.text, chat.text);
    }

    #[test]
    fn test_wire_roundtrip_festival_update() {
        let signing_key = signing::generate_signing_key();
        let update_bytes = b"fake-yrs-update";
        let engine = base64::engine::general_purpose::STANDARD;
        let sig = signing::sign(&signing_key, update_bytes);
        let signed = SignedUpdate {
            update: engine.encode(update_bytes),
            author: "organiser".to_string(),
            signature: engine.encode(&sig),
        };
        let wire = encode_gossip_message(&GossipMessage::FestivalUpdate {
            doc_id: "doc-1".to_string(),
            signed_update: signed.clone(),
        })
        .unwrap();
        assert_eq!(wire.kind, "festival_update");
        assert_eq!(wire.doc_id.as_deref(), Some("doc-1"));
        let decoded: SignedUpdate = serde_json::from_str(&wire.payload).unwrap();
        assert_eq!(decoded.author, signed.author);
    }

    #[test]
    fn test_wire_roundtrip_sync_request() {
        use crate::crypto;

        let group_key = crypto::generate_group_key();
        let fake_sv = b"fake-yrs-sv-bytes";
        let encrypted_sv = crypto::encrypt(&group_key, fake_sv).unwrap();

        let msg = GossipMessage::SyncRequest {
            doc_id: "group/doc-1".to_string(),
            encrypted_sv: encrypted_sv.clone(),
            group_key,
        };
        let wire = encode_gossip_message(&msg).unwrap();
        assert_eq!(wire.kind, "sync_request");
        assert_eq!(wire.doc_id.as_deref(), Some("group/doc-1"));
        assert!(wire.group_key_id.is_some());

        // The payload round-trips through base64.
        let decoded_sv = base64::engine::general_purpose::STANDARD
            .decode(&wire.payload)
            .unwrap();
        let plaintext = crypto::decrypt(&group_key, &decoded_sv).unwrap();
        assert_eq!(plaintext, fake_sv);

        // JSON serialise / deserialise.
        let json = serde_json::to_vec(&wire).unwrap();
        let wire2: GossipWireMessage = serde_json::from_slice(&json).unwrap();
        assert_eq!(wire2.kind, "sync_request");
        assert_eq!(wire2.doc_id, wire.doc_id);
        assert_eq!(wire2.payload, wire.payload);
    }

    #[test]
    fn test_wire_roundtrip_sync_response() {
        use crate::crypto;

        let group_key = crypto::generate_group_key();
        let fake_diff = b"fake-yrs-diff-bytes";
        let encrypted_diff = crypto::encrypt(&group_key, fake_diff).unwrap();

        let msg = GossipMessage::SyncResponse {
            doc_id: "group/doc-2".to_string(),
            encrypted_diff: encrypted_diff.clone(),
            group_key,
        };
        let wire = encode_gossip_message(&msg).unwrap();
        assert_eq!(wire.kind, "sync_response");
        assert_eq!(wire.doc_id.as_deref(), Some("group/doc-2"));

        let decoded_diff = base64::engine::general_purpose::STANDARD
            .decode(&wire.payload)
            .unwrap();
        let plaintext = crypto::decrypt(&group_key, &decoded_diff).unwrap();
        assert_eq!(plaintext, fake_diff);

        // JSON round-trip.
        let json = serde_json::to_vec(&wire).unwrap();
        let wire2: GossipWireMessage = serde_json::from_slice(&json).unwrap();
        assert_eq!(wire2.kind, "sync_response");
        assert_eq!(wire2.doc_id, wire.doc_id);
        assert_eq!(wire2.payload, wire.payload);
    }
}
