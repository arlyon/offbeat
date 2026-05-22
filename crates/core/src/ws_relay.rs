//! WebSocket relay client for communicating with the Festival Durable Object.
//!
//! The Festival DO speaks a GossipWireMessage-native JSON protocol over
//! WebSocket. This module implements a client that can connect, subscribe
//! to topics, send gossip messages, request catchup, and feed received
//! messages into the local dispatch pipeline.
//!
//! ## DO protocol
//!
//! **Client→DO:**
//! - `{ type: "auth", publicKey, attestation, signature, timestamp }`
//! - `{ type: "subscribe", topics: ["..."] }`
//! - `{ type: "unsubscribe", topics: ["..."] }`
//! - `{ type: "gossip", topic: "...", message: GossipWireMessage }`
//! - `{ type: "catchup", topic: "...", sinceSeq: 0 }`
//!
//! **DO→Client:**
//! - `{ type: "auth_ok", authenticated: true, adminCount: N }`
//! - `{ type: "subscribed", topics: [...] }`
//! - `{ type: "gossip", topic: "...", seq: N, message: GossipWireMessage }`
//! - `{ type: "catchup", topic: "...", messages: [...] }`
//! - `{ type: "error", error: "..." }`

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::auth::Attestation;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::gossip_manager::GossipWireMessage;

// ---------------------------------------------------------------------------
// Protocol types
// ---------------------------------------------------------------------------

/// Attestation payload sent in the auth message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AttestationPayload {
    pub message: String,
    pub signature: String,
    pub issuer: String,
}

impl From<Attestation> for AttestationPayload {
    fn from(att: Attestation) -> Self {
        Self {
            message: att.message,
            signature: att.signature,
            issuer: att.issuer,
        }
    }
}

/// Messages sent by the client to the DO.
#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsClientMessage {
    Auth {
        #[serde(rename = "publicKey")]
        public_key: String,
        attestation: AttestationPayload,
        signature: String,
        timestamp: String,
    },
    Subscribe {
        topics: Vec<String>,
    },
    Unsubscribe {
        topics: Vec<String>,
    },
    Gossip {
        topic: String,
        message: GossipWireMessage,
    },
    Catchup {
        topic: String,
        #[serde(rename = "sinceSeq")]
        since_seq: u64,
    },
    /// Chat catchup request using per-writer HWM state vector.
    ChatCatchup {
        topic: String,
        /// Per-writer high water marks (writer_id → last seen seq).
        sv: std::collections::HashMap<String, u64>,
        limit: u32,
    },
}

/// Messages received from the DO.
#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsServerMessage {
    AuthOk {
        authenticated: bool,
        #[serde(rename = "adminCount")]
        admin_count: u32,
    },
    Subscribed {
        topics: Vec<String>,
    },
    Gossip {
        #[allow(dead_code)]
        topic: String,
        seq: u64,
        message: GossipWireMessage,
    },
    Catchup {
        #[allow(dead_code)]
        topic: String,
        messages: Vec<CatchupEntry>,
    },
    SvDiff {
        #[serde(rename = "docId")]
        doc_id: String,
        diff: String, // base64
    },
    ChatDiff {
        topic: String,
        messages: Vec<GossipWireMessage>,
    },
    Error {
        error: String,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CatchupEntry {
    pub seq: u64,
    pub message: GossipWireMessage,
    #[allow(dead_code)]
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// WsRelaySink — cloneable send handle
// ---------------------------------------------------------------------------

type WsSink = futures_util::stream::SplitSink<
    WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsStream = futures_util::stream::SplitStream<
    WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Cloneable handle for sending messages to the Festival DO over WebSocket.
#[derive(Clone)]
pub struct WsRelaySink {
    sink: Arc<Mutex<WsSink>>,
    subscribed_topics: Arc<Mutex<HashSet<String>>>,
    last_seen_seq: Arc<Mutex<HashMap<String, u64>>>,
    authenticated: Arc<std::sync::atomic::AtomicBool>,
}

impl WsRelaySink {
    /// Send an auth message to the DO with attestation and session signature.
    pub async fn authenticate(
        &self,
        public_key_hex: &str,
        attestation: &Attestation,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> anyhow::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .to_string();
        let session_msg = format!("session:{timestamp}");
        let sig = crate::signing::sign(signing_key, session_msg.as_bytes());
        let sig_hex: String = sig.iter().map(|b| format!("{b:02x}")).collect();

        self.send_msg(&WsClientMessage::Auth {
            public_key: public_key_hex.to_string(),
            attestation: AttestationPayload::from(attestation.clone()),
            signature: sig_hex,
            timestamp,
        })
        .await
    }

    /// Whether the session has been authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.authenticated.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Send a gossip message to the DO on the given topic.
    pub async fn send_gossip(&self, topic: &str, msg: &GossipWireMessage) -> anyhow::Result<()> {
        self.send_msg(&WsClientMessage::Gossip {
            topic: topic.to_string(),
            message: msg.clone(),
        })
        .await
    }

    /// Subscribe to topics on the DO.
    pub async fn subscribe(&self, topics: Vec<String>) -> anyhow::Result<()> {
        {
            let mut subs = self.subscribed_topics.lock().await;
            for t in &topics {
                subs.insert(t.clone());
            }
        }
        self.send_msg(&WsClientMessage::Subscribe { topics }).await
    }

    /// Unsubscribe from topics on the DO.
    pub async fn unsubscribe(&self, topics: Vec<String>) -> anyhow::Result<()> {
        {
            let mut subs = self.subscribed_topics.lock().await;
            for t in &topics {
                subs.remove(t);
            }
        }
        self.send_msg(&WsClientMessage::Unsubscribe { topics }).await
    }

    /// Request catchup for a topic since a given sequence number.
    pub async fn request_catchup(&self, topic: &str, since_seq: u64) -> anyhow::Result<()> {
        self.send_msg(&WsClientMessage::Catchup {
            topic: topic.to_string(),
            since_seq,
        })
        .await
    }

    /// Send a state vector exchange request to the DO for a CRDT doc.
    ///
    /// The DO will respond with a `sv_diff` message containing the Yrs update
    /// bytes (base64) that the client is missing based on its state vector.
    pub async fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> anyhow::Result<()> {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD;
        let msg = serde_json::json!({
            "type": "sv_exchange",
            "docId": doc_id,
            "sv": b64.encode(sv),
        });
        let text = serde_json::to_string(&msg)?;
        self.sink
            .lock()
            .await
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| anyhow::anyhow!("ws send error: {e}"))
    }

    /// Get the set of topics currently subscribed to.
    pub async fn subscribed_topics(&self) -> HashSet<String> {
        self.subscribed_topics.lock().await.clone()
    }

    /// Get the last seen sequence number per topic.
    pub async fn last_seen_seq(&self) -> HashMap<String, u64> {
        self.last_seen_seq.lock().await.clone()
    }

    /// Replace the inner sink (used during reconnect).
    async fn swap_sink(&self, new_sink: WsSink) {
        let mut sink = self.sink.lock().await;
        *sink = new_sink;
    }

    async fn send_msg(&self, msg: &WsClientMessage) -> anyhow::Result<()> {
        let text = serde_json::to_string(msg)?;
        self.sink
            .lock()
            .await
            .send(Message::Text(text.into()))
            .await
            .map_err(|e| anyhow::anyhow!("ws send error: {e}"))
    }

    /// Request chat messages since the given state vector.
    ///
    /// The state vector is a map of writer_id → last seen sequence number.
    pub async fn chat_catchup(
        &self,
        topic: &str,
        sv: &crate::sync::ChatStateVector,
        limit: u32,
    ) -> anyhow::Result<()> {
        self.send_msg(&WsClientMessage::ChatCatchup {
            topic: topic.to_string(),
            sv: sv.writers.clone(),
            limit,
        })
        .await
    }

    /// Broadcast raw data on a topic (encodes as gossip message).
    pub async fn broadcast(&self, topic: &str, data: &[u8]) -> anyhow::Result<()> {
        // Parse the data as a GossipWireMessage
        let wire: GossipWireMessage = serde_json::from_slice(data)
            .map_err(|e| anyhow::anyhow!("broadcast: invalid wire message: {e}"))?;
        self.send_gossip(topic, &wire).await
    }
}

// ---------------------------------------------------------------------------
// PeerConnection implementation
// ---------------------------------------------------------------------------

impl crate::sync::PeerConnection for WsRelaySink {
    async fn subscribe(&self, topics: Vec<String>) -> anyhow::Result<()> {
        WsRelaySink::subscribe(self, topics).await
    }

    async fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> anyhow::Result<()> {
        WsRelaySink::sv_exchange(self, doc_id, sv).await
    }

    async fn chat_catchup(
        &self,
        topic: &str,
        sv: &crate::sync::ChatStateVector,
        limit: u32,
    ) -> anyhow::Result<()> {
        WsRelaySink::chat_catchup(self, topic, sv, limit).await
    }

    async fn broadcast(&self, topic: &str, data: &[u8]) -> anyhow::Result<()> {
        WsRelaySink::broadcast(self, topic, data).await
    }
}

// ---------------------------------------------------------------------------
// Connection
// ---------------------------------------------------------------------------

/// Connect to the Festival DO WebSocket at `url`.
///
/// Returns a cloneable sink handle and a future that runs the receive loop.
/// The caller should `tokio::spawn` the receive loop future.
pub async fn connect(
    url: &str,
    doc_manager: Arc<Mutex<DocManager>>,
    db: Arc<Database>,
    festival_public_key: [u8; 32],
    notifier: Arc<crate::notifier::ResourceNotifier>,
) -> anyhow::Result<(
    WsRelaySink,
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>,
)> {
    tracing::info!("ws_relay: connecting to {url}");
    let (ws_stream, _response) = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        connect_async(url),
    )
    .await
    .map_err(|_| anyhow::anyhow!("ws connect timed out after 10s"))??;
    tracing::info!("ws_relay: connected, splitting stream");
    let (sink, stream) = ws_stream.split();

    let relay_sink = WsRelaySink {
        sink: Arc::new(Mutex::new(sink)),
        subscribed_topics: Arc::new(Mutex::new(HashSet::new())),
        last_seen_seq: Arc::new(Mutex::new(HashMap::new())),
        authenticated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let recv_sink = relay_sink.clone();
    let recv_url = url.to_string();
    let receive_loop = Box::pin(run_receive_loop_with_reconnect(
        recv_sink,
        stream,
        recv_url,
        doc_manager,
        db,
        festival_public_key,
        notifier,
    ));

    Ok((relay_sink, receive_loop))
}

/// Connect with exponential backoff + full jitter, capped at 30s.
pub async fn connect_with_retry(
    url: &str,
    max_retries: u32,
    doc_manager: Arc<Mutex<DocManager>>,
    db: Arc<Database>,
    festival_public_key: [u8; 32],
    notifier: Arc<crate::notifier::ResourceNotifier>,
) -> anyhow::Result<(
    WsRelaySink,
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>,
)> {
    use rand::RngExt;
    const MAX_DELAY_MS: u64 = 30_000;

    for attempt in 0..max_retries {
        match connect(url, doc_manager.clone(), db.clone(), festival_public_key, notifier.clone()).await {
            Ok(result) => return Ok(result),
            Err(e) if attempt + 1 == max_retries => return Err(e),
            Err(e) => {
                tracing::warn!("ws_relay connect attempt {} failed: {e}", attempt + 1);
                let base_ms = 1000u64.saturating_mul(1u64 << attempt);
                let capped = base_ms.min(MAX_DELAY_MS);
                let jitter = rand::rng().random_range(0..=capped);
                tokio::time::sleep(std::time::Duration::from_millis(jitter)).await;
            }
        }
    }
    unreachable!()
}

// ---------------------------------------------------------------------------
// Receive loop with auto-reconnect
// ---------------------------------------------------------------------------

async fn run_receive_loop_with_reconnect(
    sink: WsRelaySink,
    initial_stream: WsStream,
    url: String,
    doc_manager: Arc<Mutex<DocManager>>,
    db: Arc<Database>,
    festival_public_key: [u8; 32],
    notifier: Arc<crate::notifier::ResourceNotifier>,
) -> anyhow::Result<()> {
    use rand::RngExt;
    const MAX_DELAY_MS: u64 = 30_000;

    let mut stream = initial_stream;
    let mut reconnect_attempt: u32 = 0;

    loop {
        let result = run_receive_loop(
            &sink,
            &mut stream,
            &doc_manager,
            &db,
            festival_public_key,
            &notifier,
        )
        .await;

        match result {
            Ok(()) => {
                tracing::info!("ws_relay: server closed connection cleanly");
            }
            Err(e) => {
                tracing::warn!("ws_relay receive error: {e}");
            }
        }

        // Reconnect with backoff
        loop {
            let base_ms = 1000u64.saturating_mul(1u64 << reconnect_attempt.min(5));
            let capped = base_ms.min(MAX_DELAY_MS);
            let jitter = rand::rng().random_range(0..=capped);
            tracing::info!(
                "ws_relay: reconnecting in {jitter}ms (attempt {})",
                reconnect_attempt + 1,
            );
            tokio::time::sleep(std::time::Duration::from_millis(jitter)).await;

            match tokio::time::timeout(
                std::time::Duration::from_secs(10),
                connect_async(&url),
            ).await {
                Ok(Ok((ws_stream, _))) => {
                    let (new_sink, new_stream) = ws_stream.split();
                    sink.swap_sink(new_sink).await;
                    stream = new_stream;
                    reconnect_attempt = 0;

                    // Re-subscribe to all previously subscribed topics
                    let topics: Vec<String> =
                        sink.subscribed_topics.lock().await.iter().cloned().collect();
                    if !topics.is_empty()
                        && let Err(e) = sink
                            .send_msg(&WsClientMessage::Subscribe {
                                topics: topics.clone(),
                            })
                            .await
                    {
                        tracing::warn!("ws_relay: re-subscribe failed: {e}");
                    }

                    // Request catchup for each topic
                    let seqs = sink.last_seen_seq.lock().await.clone();
                    for (topic, seq) in seqs {
                        if let Err(e) = sink.request_catchup(&topic, seq).await {
                            tracing::warn!("ws_relay: catchup request failed for {topic}: {e}");
                        }
                    }

                    tracing::info!("ws_relay: reconnected successfully");
                    break;
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        "ws_relay: reconnect attempt {} failed: {e}",
                        reconnect_attempt + 1,
                    );
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                }
                Err(_) => {
                    tracing::warn!(
                        "ws_relay: reconnect attempt {} timed out",
                        reconnect_attempt + 1,
                    );
                    reconnect_attempt = reconnect_attempt.saturating_add(1);
                }
            }
        }
    }
}

async fn run_receive_loop(
    sink: &WsRelaySink,
    stream: &mut WsStream,
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
    festival_public_key: [u8; 32],
    notifier: &Arc<crate::notifier::ResourceNotifier>,
) -> anyhow::Result<()> {
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                tracing::info!("ws_relay: recv {} bytes: {}",
                    text.len(),
                    if text.len() > 200 { &text[..200] } else { &text }
                );
                match serde_json::from_str::<WsServerMessage>(&text) {
                    Ok(server_msg) => {
                        if let Err(e) = handle_server_message(
                            server_msg,
                            sink,
                            doc_manager,
                            db,
                            festival_public_key,
                            notifier,
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
                return Err(anyhow::anyhow!("ws_relay receive error: {e}"));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

async fn handle_server_message(
    msg: WsServerMessage,
    sink: &WsRelaySink,
    doc_manager: &Arc<Mutex<DocManager>>,
    db: &Arc<Database>,
    festival_public_key: [u8; 32],
    notifier: &Arc<crate::notifier::ResourceNotifier>,
) -> anyhow::Result<()> {
    match msg {
        WsServerMessage::AuthOk {
            authenticated,
            admin_count,
        } => {
            sink.authenticated
                .store(authenticated, std::sync::atomic::Ordering::Relaxed);
            tracing::info!(
                "ws_relay: auth result: authenticated={authenticated}, admin_count={admin_count}"
            );
            Ok(())
        }
        WsServerMessage::Gossip {
            seq, message, topic,
        } => {
            // Track last seen seq for catchup on reconnect
            {
                let mut seqs = sink.last_seen_seq.lock().await;
                let entry = seqs.entry(topic.clone()).or_insert(0);
                if seq > *entry {
                    *entry = seq;
                }
            }

            // Dispatch through the standard gossip pipeline
            let wire_bytes = serde_json::to_vec(&message)?;
            crate::gossip_manager::handle_wire_bytes_pub(
                &wire_bytes,
                doc_manager,
                db,
                festival_public_key,
            )
            .await
        }
        WsServerMessage::Catchup { messages, topic } => {
            for entry in messages {
                // Track seq
                {
                    let mut seqs = sink.last_seen_seq.lock().await;
                    let current = seqs.entry(topic.clone()).or_insert(0);
                    if entry.seq > *current {
                        *current = entry.seq;
                    }
                }

                let wire_bytes = serde_json::to_vec(&entry.message)?;
                if let Err(e) = crate::gossip_manager::handle_wire_bytes_pub(
                    &wire_bytes,
                    doc_manager,
                    db,
                    festival_public_key,
                )
                .await
                {
                    tracing::warn!("ws_relay catchup dispatch error: {e}");
                }
            }
            Ok(())
        }
        WsServerMessage::SvDiff { doc_id, diff } => {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD;
            let diff_bytes = b64
                .decode(&diff)
                .map_err(|e| anyhow::anyhow!("sv_diff base64 decode: {e}"))?;
            let mut dm = doc_manager.lock().await;
            dm.apply_update(&doc_id, &diff_bytes)?;
            tracing::info!(
                "ws_relay: applied sv_diff for {doc_id} ({} bytes)",
                diff_bytes.len()
            );
            // Notify watchers that the doc was updated
            notifier.notify_doc(&doc_id);
            Ok(())
        }
        WsServerMessage::ChatDiff { topic, messages } => {
            for wire_msg in messages {
                let wire_bytes = serde_json::to_vec(&wire_msg)?;
                if let Err(e) = crate::gossip_manager::handle_wire_bytes_pub(
                    &wire_bytes,
                    doc_manager,
                    db,
                    festival_public_key,
                )
                .await
                {
                    tracing::warn!("ws_relay chat_diff dispatch error: {e}");
                }
            }
            tracing::info!("ws_relay: applied chat_diff for topic {topic}");
            // Notify watchers that chat was updated
            notifier.notify_chat(&topic);
            Ok(())
        }
        WsServerMessage::Subscribed { topics } => {
            tracing::info!("ws_relay: subscribed to topics: {:?}", topics);
            Ok(())
        }
        WsServerMessage::Error { error } => {
            tracing::warn!("ws_relay: server error: {error}");
            Ok(())
        }
        WsServerMessage::Unknown => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that retry backoff delays increase exponentially and are capped.
    #[test]
    fn test_retry_backoff_timing() {
        use rand::RngExt;
        const MAX_DELAY_MS: u64 = 30_000;

        let mut rng = rand::rng();
        let mut prev_cap = 0u64;

        for attempt in 0u32..8 {
            let base_ms = 1000u64.saturating_mul(1u64 << attempt);
            let capped = base_ms.min(MAX_DELAY_MS);
            let jitter = rng.random_range(0..=capped);

            assert!(capped >= prev_cap, "cap should not decrease");
            assert!(jitter <= capped, "jitter {jitter} exceeded cap {capped}");
            if attempt >= 5 {
                assert_eq!(capped, MAX_DELAY_MS, "should be capped at 30s by attempt {attempt}");
            }
            prev_cap = capped;
        }
    }

    #[test]
    fn test_serialize_subscribe() {
        let msg = WsClientMessage::Subscribe {
            topics: vec!["festival/test/chat".to_string()],
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"subscribe\""));
        assert!(json.contains("festival/test/chat"));
    }

    #[test]
    fn test_serialize_gossip() {
        let wire = GossipWireMessage {
            kind: "chat".to_string(),
            doc_id: None,
            payload: r#"{"id":"c1","userId":"u1","displayName":"Alice","text":"hi","topic":"t","timestamp":"now"}"#.to_string(),
            group_key_id: None,
        };
        let msg = WsClientMessage::Gossip {
            topic: "festival/test/chat".to_string(),
            message: wire,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"gossip\""));
        assert!(json.contains("\"kind\":\"chat\""));
    }

    #[test]
    fn test_serialize_catchup() {
        let msg = WsClientMessage::Catchup {
            topic: "festival/test/state".to_string(),
            since_seq: 42,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"type\":\"catchup\""));
        assert!(json.contains("\"sinceSeq\":42"));
    }

    #[test]
    fn test_deserialize_server_gossip() {
        let raw = r#"{
            "type": "gossip",
            "topic": "festival/test/chat",
            "seq": 42,
            "message": {
                "kind": "chat",
                "payload": "{\"id\":\"m1\"}",
                "group_key_id": null
            }
        }"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::Gossip { seq, message, .. } => {
                assert_eq!(seq, 42);
                assert_eq!(message.kind, "chat");
            }
            other => panic!("expected Gossip, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_server_catchup() {
        let raw = r#"{
            "type": "catchup",
            "topic": "festival/test/state",
            "messages": [
                {
                    "seq": 1,
                    "message": { "kind": "chat", "payload": "{}", "group_key_id": null },
                    "timestamp": "2026-01-01"
                }
            ]
        }"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::Catchup { messages, .. } => {
                assert_eq!(messages.len(), 1);
                assert_eq!(messages[0].seq, 1);
            }
            other => panic!("expected Catchup, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_server_subscribed() {
        let raw = r#"{"type":"subscribed","topics":["festival/test/chat"]}"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::Subscribed { topics } => {
                assert_eq!(topics, vec!["festival/test/chat"]);
            }
            other => panic!("expected Subscribed, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_server_error() {
        let raw = r#"{"type":"error","error":"bad request"}"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::Error { error } => {
                assert_eq!(error, "bad request");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_unknown_type() {
        let raw = r#"{"type":"future_type","data":"something"}"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        assert!(matches!(msg, WsServerMessage::Unknown));
    }

    #[test]
    fn test_deserialize_sv_diff() {
        let raw = r#"{"type":"sv_diff","docId":"festival/test/state","diff":"AQAAAA=="}"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::SvDiff { doc_id, diff } => {
                assert_eq!(doc_id, "festival/test/state");
                assert_eq!(diff, "AQAAAA==");
            }
            other => panic!("expected SvDiff, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_auth_ok() {
        let raw = r#"{"type":"auth_ok","authenticated":true,"adminCount":1}"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::AuthOk { authenticated, admin_count } => {
                assert!(authenticated);
                assert_eq!(admin_count, 1);
            }
            other => panic!("expected AuthOk, got {other:?}"),
        }
    }

    #[test]
    fn test_deserialize_chat_diff() {
        let raw = r#"{"type":"chat_diff","topic":"festival/test/chat","messages":[]}"#;
        let msg: WsServerMessage = serde_json::from_str(raw).unwrap();
        match msg {
            WsServerMessage::ChatDiff { topic, messages } => {
                assert_eq!(topic, "festival/test/chat");
                assert!(messages.is_empty());
            }
            other => panic!("expected ChatDiff, got {other:?}"),
        }
    }
}
