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
                    // Group-key messages cannot be dispatched without the
                    // local key store; log and skip for now.
                    if wire.kind == "group_update" || wire.kind == "encrypted_chat" {
                        tracing::warn!(
                            "ws_relay: skipping {} — group key lookup not yet wired",
                            wire.kind
                        );
                    }
                    // Other wire kinds (festival_update, chat) could be
                    // handled here if needed; for now the primary path is
                    // through the gossip layer.
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
            // Relay catchup entries: same treatment as live relay events.
            drop(dm);
            for entry in relay {
                let raw = base64::engine::general_purpose::STANDARD
                    .decode(&entry.data)
                    .map_err(|e| anyhow::anyhow!("base64 decode relay catchup: {e}"))?;
                if let Ok(wire) =
                    serde_json::from_slice::<crate::gossip_manager::GossipWireMessage>(&raw)
                {
                    tracing::debug!("ws_relay catchup: relay kind={}", wire.kind);
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
