use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt;
use iroh::EndpointId;
use iroh_gossip::api::Event;
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
}

// ---------------------------------------------------------------------------
// Wire format — serialised over gossip / WS relay
// ---------------------------------------------------------------------------

/// Flat wire message that travels over iroh-gossip or the WS relay.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct GossipWireMessage {
    /// Discriminator: "festival_update" | "group_update" | "chat" | "encrypted_chat"
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
    festival_public_key: Option<&[u8; 32]>,
) -> anyhow::Result<()> {
    match msg {
        GossipMessage::FestivalUpdate {
            doc_id,
            signed_update,
        } => {
            let pk = festival_public_key
                .ok_or_else(|| anyhow::anyhow!("no festival public key provided"))?;
            doc_manager.apply_signed_update(&doc_id, &signed_update, pk)?;
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
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// GossipManager — iroh-gossip networking layer
// ---------------------------------------------------------------------------

/// Manages iroh-gossip subscriptions and routes incoming events to
/// `dispatch_message`.
pub struct GossipManager {
    gossip: Gossip,
    /// Active topic handles, keyed by TopicId.
    subscriptions: HashMap<TopicId, iroh_gossip::api::GossipSender>,
    doc_manager: Arc<Mutex<DocManager>>,
    db: Arc<Database>,
}

impl GossipManager {
    pub fn new(gossip: Gossip, doc_manager: Arc<Mutex<DocManager>>, db: Arc<Database>) -> Self {
        Self {
            gossip,
            subscriptions: HashMap::new(),
            doc_manager,
            db,
        }
    }

    /// Join a gossip topic and store a sender handle for later broadcasts.
    ///
    /// `bootstrap` is the list of peer endpoint IDs already on the topic.
    pub async fn subscribe(
        &mut self,
        topic_id: TopicId,
        bootstrap: Vec<EndpointId>,
    ) -> anyhow::Result<()> {
        let topic = self.gossip.subscribe(topic_id, bootstrap).await?;
        let (sender, mut receiver) = topic.split();
        self.subscriptions.insert(topic_id, sender);

        // Spawn a background task that drains events for this topic.
        let doc_manager = Arc::clone(&self.doc_manager);
        let db = Arc::clone(&self.db);
        tokio::spawn(async move {
            while let Some(event) = receiver.next().await {
                match event {
                    Ok(Event::Received(msg)) => {
                        if let Err(e) =
                            handle_wire_bytes(&msg.content, &doc_manager, &db, None).await
                        {
                            tracing::warn!("gossip dispatch error: {e}");
                        }
                    }
                    Ok(Event::NeighborUp(id)) => {
                        tracing::debug!("neighbor up: {id}");
                    }
                    Ok(Event::NeighborDown(id)) => {
                        tracing::debug!("neighbor down: {id}");
                    }
                    Ok(Event::Lagged) => {
                        tracing::warn!("gossip receiver lagged — some messages were dropped");
                    }
                    Err(e) => {
                        tracing::warn!("gossip receiver error: {e}");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Leave a previously joined gossip topic.
    pub async fn unsubscribe(&mut self, topic_id: TopicId) -> anyhow::Result<()> {
        // Dropping the sender causes the gossip actor to leave the topic.
        self.subscriptions.remove(&topic_id);
        Ok(())
    }

    /// Broadcast raw bytes to all peers on the given topic.
    pub async fn publish(&mut self, topic_id: TopicId, data: Vec<u8>) -> anyhow::Result<()> {
        let sender = self
            .subscriptions
            .get_mut(&topic_id)
            .ok_or_else(|| anyhow::anyhow!("not subscribed to topic {topic_id:?}"))?;
        sender.broadcast(Bytes::from(data)).await?;
        Ok(())
    }

    /// Encode a `GossipMessage` as a `GossipWireMessage` and broadcast it.
    pub async fn publish_message(
        &mut self,
        topic_id: TopicId,
        msg: &GossipMessage,
    ) -> anyhow::Result<()> {
        let wire = encode_gossip_message(msg)?;
        let bytes = serde_json::to_vec(&wire)?;
        self.publish(topic_id, bytes).await
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
    }
}

/// Decode a `GossipWireMessage` from raw bytes and dispatch it.
///
/// `festival_public_key` is needed only for `festival_update` messages.
async fn handle_wire_bytes(
    raw: &[u8],
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
    festival_public_key: Option<[u8; 32]>,
) -> anyhow::Result<()> {
    let wire: GossipWireMessage = serde_json::from_slice(raw)
        .map_err(|e| anyhow::anyhow!("deserialise wire message: {e}"))?;

    let gossip_msg = match wire.kind.as_str() {
        "festival_update" => {
            let signed_update: SignedUpdate = serde_json::from_str(&wire.payload)?;
            GossipMessage::FestivalUpdate {
                doc_id: wire
                    .doc_id
                    .ok_or_else(|| anyhow::anyhow!("festival_update missing doc_id"))?,
                signed_update,
            }
        }
        "group_update" => {
            // NOTE: The group key itself cannot be reconstructed from the key_id alone.
            // In a real node the caller would look up the key from local storage.
            anyhow::bail!(
                "group_update requires group key lookup (key_id={:?}); not yet implemented in wire path",
                wire.group_key_id
            );
        }
        "chat" => {
            let chat: ChatMessage = serde_json::from_str(&wire.payload)?;
            GossipMessage::Chat(chat)
        }
        "encrypted_chat" => {
            anyhow::bail!(
                "encrypted_chat requires group key lookup (key_id={:?}); not yet implemented in wire path",
                wire.group_key_id
            );
        }
        other => anyhow::bail!("unknown gossip message kind: {other}"),
    };

    let mut dm = doc_manager.lock().await;
    dispatch_message(&mut dm, db, gossip_msg, festival_public_key.as_ref())
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
        };

        dispatch_message(
            &mut doc_mgr,
            &db_arc,
            GossipMessage::Chat(msg.clone()),
            None,
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
            Some(&public_key),
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

        dispatch_message(
            &mut doc_mgr,
            &db_arc,
            GossipMessage::GroupUpdate {
                doc_id: "group-doc".to_string(),
                encrypted,
                group_key,
            },
            None,
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
}
