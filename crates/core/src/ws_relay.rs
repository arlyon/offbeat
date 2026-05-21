//! WebSocket relay client for communicating with the Festival Durable Object.
//!
//! The Festival DO speaks a JSON-over-WebSocket protocol. This module
//! implements a client that can connect, subscribe to topics, send chat and
//! relay messages, and feed received messages into the local dispatch
//! pipeline.
//!
//! ## DO protocol (from festival-do.ts)
//!
//! **Send:**
//! - `{ type: "subscribe", topics: ["..."] }`
//! - `{ type: "chat", topic: "...", message: { id, userId, displayName, text, topic, timestamp } }`
//! - `{ type: "relay", topic: "...", data: "base64..." }`
//! - `{ type: "catchup", topic: "...", sinceSeq: 0 }`
//!
//! **Receive:**
//! - `{ type: "subscribed", topics: [...] }`
//! - `{ type: "chat", topic: "...", seq: N, message: {...} }`
//! - `{ type: "relay", topic: "...", seq: N, data: "base64..." }`
//! - `{ type: "catchup", topic: "...", chat: [...], relay: [...] }`

use std::sync::Arc;

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::gossip_manager::dispatch_message;
use crate::types::ChatMessage;

// ---------------------------------------------------------------------------
// Protocol types
// ---------------------------------------------------------------------------

/// Messages sent by the client to the DO.
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DoClientMessage {
    Subscribe {
        topics: Vec<String>,
    },
    Unsubscribe {
        topics: Vec<String>,
    },
    Chat {
        topic: String,
        message: ChatMessage,
    },
    Relay {
        topic: String,
        data: String, // base64
    },
    Catchup {
        topic: String,
        #[serde(rename = "sinceSeq")]
        since_seq: u64,
    },
}

/// Messages received from the DO.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DoServerMessage {
    Subscribed {
        topics: Vec<String>,
    },
    Chat {
        topic: String,
        seq: u64,
        message: ChatMessage,
    },
    Relay {
        topic: String,
        seq: u64,
        data: String, // base64
    },
    Catchup {
        topic: String,
        chat: Vec<CatchupChatEntry>,
        relay: Vec<CatchupRelayEntry>,
    },
    Error {
        error: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CatchupChatEntry {
    pub seq: u64,
    pub message: ChatMessage,
    pub timestamp: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CatchupRelayEntry {
    pub seq: u64,
    pub data: String, // base64
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// WsRelay struct
// ---------------------------------------------------------------------------

type WsSink = futures_util::stream::SplitSink<
    WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsStream = futures_util::stream::SplitStream<
    WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// A live WebSocket connection to a Festival Durable Object.
pub struct WsRelay {
    sink: WsSink,
    stream: Option<WsStream>,
}

impl WsRelay {
    // -----------------------------------------------------------------------
    // Construction
    // -----------------------------------------------------------------------

    /// Establish a WebSocket connection to `url`.
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let (ws_stream, _response) = connect_async(url).await?;
        let (sink, stream) = ws_stream.split();
        Ok(Self {
            sink,
            stream: Some(stream),
        })
    }

    // -----------------------------------------------------------------------
    // Sending
    // -----------------------------------------------------------------------

    async fn send_msg(&mut self, msg: &DoClientMessage) -> anyhow::Result<()> {
        let text = serde_json::to_string(msg)?;
        self.sink
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| anyhow::anyhow!("ws send error: {e}"))
    }

    /// Send a `subscribe` message to the DO for the given topic names.
    pub async fn subscribe(&mut self, topics: Vec<String>) -> anyhow::Result<()> {
        self.send_msg(&DoClientMessage::Subscribe { topics }).await
    }

    /// Send an `unsubscribe` message.
    pub async fn unsubscribe(&mut self, topics: Vec<String>) -> anyhow::Result<()> {
        self.send_msg(&DoClientMessage::Unsubscribe { topics }).await
    }

    /// Send a chat message to the DO on the given topic.
    pub async fn send_chat(&mut self, topic: &str, message: &ChatMessage) -> anyhow::Result<()> {
        self.send_msg(&DoClientMessage::Chat {
            topic: topic.to_string(),
            message: message.clone(),
        })
        .await
    }

    /// Send a relay (binary CRDT update) message. `data` is raw bytes; this
    /// method base64-encodes them before sending.
    pub async fn send_relay(&mut self, topic: &str, data: &[u8]) -> anyhow::Result<()> {
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        self.send_msg(&DoClientMessage::Relay {
            topic: topic.to_string(),
            data: encoded,
        })
        .await
    }

    /// Request a catchup for the given topic since `since_seq`.
    pub async fn request_catchup(&mut self, topic: &str, since_seq: u64) -> anyhow::Result<()> {
        self.send_msg(&DoClientMessage::Catchup {
            topic: topic.to_string(),
            since_seq,
        })
        .await
    }

    // -----------------------------------------------------------------------
    // Receiving
    // -----------------------------------------------------------------------

    /// Take the receive half out of this handle and run an event loop that
    /// feeds incoming messages into the dispatch pipeline.
    ///
    /// This method consumes `self.stream`; calling it twice will panic.
    ///
    /// `festival_public_key` is forwarded to `dispatch_message` for
    /// verifying signed festival CRDT updates.
    pub async fn run_receive_loop(
        &mut self,
        doc_manager: Arc<Mutex<DocManager>>,
        db: Arc<Database>,
        festival_public_key: Option<[u8; 32]>,
    ) -> anyhow::Result<()> {
        let mut stream = self
            .stream
            .take()
            .ok_or_else(|| anyhow::anyhow!("receive loop already started"))?;

        while let Some(msg) = stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    match serde_json::from_str::<DoServerMessage>(&text) {
                        Ok(server_msg) => {
                            if let Err(e) = handle_server_message(
                                server_msg,
                                &doc_manager,
                                &db,
                                festival_public_key,
                            )
                            .await
                            {
                                tracing::warn!("ws_relay dispatch error: {e}");
                            }
                        }
                        Err(e) => {
                            tracing::warn!("ws_relay deserialise error: {e} — raw: {text}");
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    tracing::info!("ws_relay: server closed connection");
                    break;
                }
                Ok(_) => {} // binary / ping / pong — ignore
                Err(e) => {
                    tracing::warn!("ws_relay receive error: {e}");
                    break;
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Dispatch helpers
// ---------------------------------------------------------------------------

async fn handle_server_message(
    msg: DoServerMessage,
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
    festival_public_key: Option<[u8; 32]>,
) -> anyhow::Result<()> {
    match msg {
        DoServerMessage::Chat { message, .. } => {
            let mut dm = doc_manager.lock().await;
            dispatch_message(
                &mut dm,
                db,
                crate::gossip_manager::GossipMessage::Chat(message),
                festival_public_key.as_ref(),
            )
        }
        DoServerMessage::Relay { data, .. } => {
            // The relay data is a base64-encoded GossipWireMessage (or raw
            // CRDT bytes depending on the sender). We attempt to decode it as
            // a GossipWireMessage; if that fails we log and skip.
            let raw = base64::engine::general_purpose::STANDARD
                .decode(&data)
                .map_err(|e| anyhow::anyhow!("base64 decode relay: {e}"))?;

            // Try to interpret as a GossipWireMessage.
            match serde_json::from_slice::<crate::gossip_manager::GossipWireMessage>(&raw) {
                Ok(wire) => {
                    tracing::debug!("ws_relay: relay kind={}", wire.kind);
                    match wire.kind.as_str() {
                        "sync_response" | "sync_update" => {
                            use base64::Engine as _;
                            let b64 = base64::engine::general_purpose::STANDARD;

                            if let (Some(key_id), Some(doc_id)) =
                                (&wire.group_key_id, &wire.doc_id)
                            {
                                match db.load_group_key(key_id) {
                                    Ok(Some(group_key)) => {
                                        match b64.decode(&wire.payload) {
                                            Ok(encrypted) => {
                                                let mut dm = doc_manager.lock().await;
                                                if let Err(e) = dm.apply_encrypted_update(
                                                    doc_id,
                                                    &encrypted,
                                                    &group_key,
                                                ) {
                                                    tracing::warn!(
                                                        "ws_relay: {} apply error: {e}",
                                                        wire.kind
                                                    );
                                                }
                                            }
                                            Err(e) => tracing::warn!(
                                                "ws_relay: {} base64 decode: {e}",
                                                wire.kind
                                            ),
                                        }
                                    }
                                    Ok(None) => tracing::debug!(
                                        "ws_relay: {} unknown key_id={key_id}",
                                        wire.kind
                                    ),
                                    Err(e) => tracing::warn!(
                                        "ws_relay: {} key lookup error: {e}",
                                        wire.kind
                                    ),
                                }
                            } else {
                                tracing::warn!(
                                    "ws_relay: {} missing group_key_id or doc_id",
                                    wire.kind
                                );
                            }
                        }
                        "sync_request" => {
                            // A peer is requesting a sync.  Responding requires
                            // the group key and a write-capable reference back to
                            // the sink — not available in this read-only handler.
                            // Higher-level integration code should intercept these.
                            tracing::debug!(
                                "ws_relay: received sync_request for doc {:?} — response not handled here",
                                wire.doc_id
                            );
                        }
                        "group_update" => {
                            use base64::Engine as _;
                            let b64 = base64::engine::general_purpose::STANDARD;

                            if let Some(key_id) = &wire.group_key_id {
                                match db.load_group_key(key_id) {
                                    Ok(Some(group_key)) => {
                                        match b64.decode(&wire.payload) {
                                            Ok(encrypted) => {
                                                if let Some(doc_id) = &wire.doc_id {
                                                    let mut dm = doc_manager.lock().await;
                                                    if let Err(e) = dm.apply_encrypted_update(
                                                        doc_id,
                                                        &encrypted,
                                                        &group_key,
                                                    ) {
                                                        tracing::warn!(
                                                            "ws_relay: group_update apply error: {e}"
                                                        );
                                                    }
                                                }
                                            }
                                            Err(e) => tracing::warn!(
                                                "ws_relay: group_update base64 decode: {e}"
                                            ),
                                        }
                                    }
                                    Ok(None) => tracing::debug!(
                                        "ws_relay: group_update unknown key_id={key_id}"
                                    ),
                                    Err(e) => tracing::warn!(
                                        "ws_relay: group_update key lookup error: {e}"
                                    ),
                                }
                            } else {
                                tracing::warn!("ws_relay: group_update missing group_key_id");
                            }
                        }

                        "encrypted_chat" => {
                            use base64::Engine as _;
                            let b64 = base64::engine::general_purpose::STANDARD;

                            if let Some(key_id) = &wire.group_key_id {
                                match db.load_group_key(key_id) {
                                    Ok(Some(group_key)) => {
                                        match b64.decode(&wire.payload) {
                                            Ok(encrypted) => {
                                                match crate::crypto::decrypt(&group_key, &encrypted) {
                                                    Ok(plaintext) => {
                                                        match serde_json::from_slice::<ChatMessage>(
                                                            &plaintext,
                                                        ) {
                                                            Ok(chat) => {
                                                                if let Err(e) =
                                                                    db.save_chat_message(&chat)
                                                                {
                                                                    tracing::warn!(
                                                                        "ws_relay: encrypted_chat save error: {e}"
                                                                    );
                                                                }
                                                            }
                                                            Err(e) => tracing::warn!(
                                                                "ws_relay: encrypted_chat deserialise: {e}"
                                                            ),
                                                        }
                                                    }
                                                    Err(e) => tracing::warn!(
                                                        "ws_relay: encrypted_chat decrypt error: {e}"
                                                    ),
                                                }
                                            }
                                            Err(e) => tracing::warn!(
                                                "ws_relay: encrypted_chat base64 decode: {e}"
                                            ),
                                        }
                                    }
                                    Ok(None) => tracing::debug!(
                                        "ws_relay: encrypted_chat unknown key_id={key_id}"
                                    ),
                                    Err(e) => tracing::warn!(
                                        "ws_relay: encrypted_chat key lookup error: {e}"
                                    ),
                                }
                            } else {
                                tracing::warn!("ws_relay: encrypted_chat missing group_key_id");
                            }
                        }
                        other => {
                            tracing::debug!("ws_relay: relay kind={other} — no special handling");
                        }
                    }
                }
                Err(_) => {
                    tracing::debug!("ws_relay: relay payload is not a GossipWireMessage — ignoring");
                }
            }
            Ok(())
        }
        DoServerMessage::Catchup { chat, relay, .. } => {
            let mut dm = doc_manager.lock().await;
            for entry in chat {
                dispatch_message(
                    &mut dm,
                    db,
                    crate::gossip_manager::GossipMessage::Chat(entry.message),
                    festival_public_key.as_ref(),
                )?;
            }
            // Relay catchup entries: apply group-keyed updates if we have the key.
            drop(dm);
            for entry in relay {
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(&entry.data)
                    .map_err(|e| anyhow::anyhow!("base64 decode relay catchup: {e}"))?;
                if let Ok(wire) =
                    serde_json::from_slice::<crate::gossip_manager::GossipWireMessage>(&raw)
                {
                    tracing::debug!("ws_relay catchup: relay kind={}", wire.kind);
                    apply_relay_wire_catchup(wire, doc_manager, db).await;
                }
            }
            Ok(())
        }
        DoServerMessage::Subscribed { topics } => {
            tracing::info!("ws_relay: subscribed to topics: {:?}", topics);
            Ok(())
        }
        DoServerMessage::Error { error } => {
            tracing::warn!("ws_relay: server error: {error}");
            Ok(())
        }
        DoServerMessage::Unknown => Ok(()),
    }
}

/// Apply a single wire message received in a relay catchup, using DB key
/// lookup for group-encrypted payloads.  Errors are logged and ignored so
/// that one bad entry does not abort the whole catchup loop.
async fn apply_relay_wire_catchup(
    wire: crate::gossip_manager::GossipWireMessage,
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
) {
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD;

    match wire.kind.as_str() {
        "group_update" | "sync_response" | "sync_update" => {
            let (Some(key_id), Some(doc_id)) = (&wire.group_key_id, &wire.doc_id) else {
                return;
            };
            let group_key = match db.load_group_key(key_id) {
                Ok(Some(k)) => k,
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!("ws_relay catchup: key lookup error: {e}");
                    return;
                }
            };
            let encrypted = match b64.decode(&wire.payload) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("ws_relay catchup: base64 decode: {e}");
                    return;
                }
            };
            let mut dm = doc_manager.lock().await;
            if let Err(e) = dm.apply_encrypted_update(doc_id, &encrypted, &group_key) {
                tracing::warn!("ws_relay catchup: {} apply error: {e}", wire.kind);
            }
        }
        "encrypted_chat" => {
            let Some(key_id) = &wire.group_key_id else {
                return;
            };
            let group_key = match db.load_group_key(key_id) {
                Ok(Some(k)) => k,
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!("ws_relay catchup: key lookup error: {e}");
                    return;
                }
            };
            let encrypted = match b64.decode(&wire.payload) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("ws_relay catchup: base64 decode: {e}");
                    return;
                }
            };
            let plaintext = match crate::crypto::decrypt(&group_key, &encrypted) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("ws_relay catchup: encrypted_chat decrypt: {e}");
                    return;
                }
            };
            match serde_json::from_slice::<ChatMessage>(&plaintext) {
                Ok(chat) => {
                    if let Err(e) = db.save_chat_message(&chat) {
                        tracing::warn!("ws_relay catchup: encrypted_chat save: {e}");
                    }
                }
                Err(e) => {
                    tracing::warn!("ws_relay catchup: encrypted_chat deserialise: {e}");
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_subscribe() {
        let msg = DoClientMessage::Subscribe {
            topics: vec!["festival/test/chat".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribe\""));
        assert!(json.contains("festival/test/chat"));
    }

    #[test]
    fn test_serialize_chat() {
        let chat = ChatMessage {
            id: "c1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "hi".to_string(),
            topic: "festival/test/chat".to_string(),
            stage_id: None,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
        };
        let msg = DoClientMessage::Chat {
            topic: "festival/test/chat".to_string(),
            message: chat,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"chat\""));
        assert!(json.contains("\"text\":\"hi\""));
    }

    #[test]
    fn test_deserialize_server_chat() {
        let raw = r#"{
            "type": "chat",
            "topic": "festival/test/chat",
            "seq": 42,
            "message": {
                "id": "m1",
                "userId": "u1",
                "displayName": "Alice",
                "text": "hello",
                "topic": "festival/test/chat",
                "timestamp": "2026-01-01T00:00:00Z"
            }
        }"#;
        let msg: DoServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            DoServerMessage::Chat { seq, message, .. } => {
                assert_eq!(seq, 42);
                assert_eq!(message.text, "hello");
            }
            other => panic!("expected Chat, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_server_relay() {
        let data = base64::engine::general_purpose::STANDARD.encode(b"crdt-bytes");
        let raw = format!(
            r#"{{"type":"relay","topic":"festival/test/state","seq":1,"data":"{data}"}}"#
        );
        let msg: DoServerMessage = serde_json::from_str(&raw).unwrap();
        match msg {
            DoServerMessage::Relay { seq, data: d, .. } => {
                assert_eq!(seq, 1);
                assert_eq!(d, data);
            }
            other => panic!("expected Relay, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_server_subscribed() {
        let raw = r#"{"type":"subscribed","topics":["festival/test/chat"]}"#;
        let msg: DoServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            DoServerMessage::Subscribed { topics } => {
                assert_eq!(topics, vec!["festival/test/chat"]);
            }
            other => panic!("expected Subscribed, got {other:?}"),
        }
    }

    #[test]
    fn test_serialize_relay() {
        let data = b"hello crdt";
        let encoded = base64::engine::general_purpose::STANDARD.encode(data);
        let msg = DoClientMessage::Relay {
            topic: "festival/test/state".to_string(),
            data: encoded.clone(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"relay\""));
        assert!(json.contains(&encoded));
    }
}
