//! WebSocket relay client for communicating with the Festival Durable Object.
//!
//! The Festival DO speaks a protobuf binary protocol over WebSocket.
//! This module implements a client that can connect, subscribe to topics,
//! send gossip messages, request catchup, and feed received messages
//! into the local dispatch pipeline.
//!
//! ## Gossip Protocol Bridge
//!
//! The DO cannot speak QUIC (CF Workers limitation), so WebSocket is the
//! transport. However, the wire format is the standard GossipEnvelope protobuf.
//! On connection, the DO sends a `RelayHello` message containing its
//! deterministic `endpoint_id` (hex-encoded Ed25519 public key). The client
//! stores this and registers the DO as a known peer in the `ConnectionManager`,
//! enabling the gossip topology to treat the WS relay as a regular peer.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use prost::Message;
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite};

use crate::auth::Attestation;
use crate::connection_manager::ConnectionManager;
use crate::doc_manager::DocManager;
use crate::proto;
use crate::sync::SyncOrchestrator;

fn is_public_chat_topic(topic: &str) -> bool {
    let mut parts = topic.split('/');
    matches!(
        (parts.next(), parts.next(), parts.next(), parts.next()),
        (Some("festival"), Some(_), Some("chat"), Some(_))
    )
}

// ---------------------------------------------------------------------------
// WsRelaySink — cloneable send handle
// ---------------------------------------------------------------------------

type WsSink = futures_util::stream::SplitSink<
    WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Message,
>;
type WsStream =
    futures_util::stream::SplitStream<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>>;
type GossipAcknowledgementWaiters = HashMap<[u8; 32], Vec<oneshot::Sender<()>>>;
const RELAY_ACK_CAPABILITY_TOPIC: &str = "__offbeat/relay-ack/v1";

async fn read_relay_hello(
    stream: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
) -> anyhow::Result<proto::RelayHello> {
    let frame = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .map_err(|_| anyhow::anyhow!("relay hello timed out"))?
        .ok_or_else(|| anyhow::anyhow!("relay closed before hello"))??;
    let tungstenite::Message::Binary(bytes) = frame else {
        anyhow::bail!("relay hello must be a binary frame");
    };
    let message = proto::RelayServerMessage::decode(bytes.as_ref())?;
    match message.msg {
        Some(proto::relay_server_message::Msg::Hello(hello)) => Ok(hello),
        _ => anyhow::bail!("relay did not send hello first"),
    }
}

#[derive(Clone)]
struct ReconnectAuth {
    public_key_hex: String,
    attestation: Attestation,
    signing_key: ed25519_dalek::SigningKey,
}

#[derive(Clone)]
struct RelayAuthChallenge {
    bytes: Vec<u8>,
    festival_id: String,
    expires_at: u64,
}

/// Cloneable handle for sending messages to the Festival DO over WebSocket.
#[derive(Clone)]
pub struct WsRelaySink {
    sink: Arc<Mutex<WsSink>>,
    subscribed_topics: Arc<Mutex<HashSet<String>>>,
    last_seen_seq: Arc<Mutex<HashMap<String, u64>>>,
    authenticated: Arc<std::sync::atomic::AtomicBool>,
    reconnect_auth: Arc<Mutex<Option<ReconnectAuth>>>,
    acknowledgement_waiters: Arc<Mutex<GossipAcknowledgementWaiters>>,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
    shutdown_notify: Arc<tokio::sync::Notify>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    tx_bytes: Arc<std::sync::atomic::AtomicU64>,
    rx_bytes: Arc<std::sync::atomic::AtomicU64>,
    /// The DO's endpoint_id, received from the Hello message on connect.
    /// This is the hex-encoded 32-byte Ed25519 public key of the Festival DO.
    do_endpoint_id: Arc<Mutex<Option<String>>>,
    auth_challenge: Arc<Mutex<Option<RelayAuthChallenge>>>,
}

impl WsRelaySink {
    /// Send an auth message to the DO with attestation and session signature.
    pub async fn authenticate(
        &self,
        public_key_hex: &str,
        attestation: &Attestation,
        signing_key: &ed25519_dalek::SigningKey,
    ) -> anyhow::Result<()> {
        let auth = ReconnectAuth {
            public_key_hex: public_key_hex.to_string(),
            attestation: attestation.clone(),
            signing_key: signing_key.clone(),
        };
        *self.reconnect_auth.lock().await = Some(auth.clone());
        self.send_auth(&auth).await
    }

    async fn send_auth(&self, auth: &ReconnectAuth) -> anyhow::Result<()> {
        let challenge = self
            .auth_challenge
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("relay auth challenge is unavailable"))?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        if now > challenge.expires_at {
            anyhow::bail!("relay auth challenge expired");
        }

        let public_key_bytes = hex_to_bytes(&auth.public_key_hex)?;
        let payload = crate::signing::relay_auth_signing_payload(
            &challenge.festival_id,
            &challenge.bytes,
            &public_key_bytes,
        )?;
        let sig = crate::signing::sign(&auth.signing_key, &payload);
        let issuer_bytes = hex_to_bytes(&auth.attestation.issuer)?;
        let att_sig_bytes = hex_to_bytes(&auth.attestation.signature)?;

        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Auth(proto::AuthRequest {
                public_key: public_key_bytes,
                attestation: Some(proto::Attestation {
                    message: auth.attestation.message.clone(),
                    signature: att_sig_bytes,
                    issuer: issuer_bytes,
                }),
                signature: sig,
                timestamp: String::new(),
                challenge: challenge.bytes,
            })),
        })
        .await
    }

    /// Stop this relay and its reconnect loop.
    pub async fn close(&self) {
        self.shutdown
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self.connected
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.authenticated
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.shutdown_notify.notify_one();
        let _ = self
            .sink
            .lock()
            .await
            .send(tungstenite::Message::Close(None))
            .await;
    }

    /// Whether the session has been authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.authenticated
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Whether the WebSocket is currently connected.
    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Cumulative bytes sent over the relay.
    pub fn tx_bytes(&self) -> u64 {
        self.tx_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Cumulative bytes received over the relay.
    pub fn rx_bytes(&self) -> u64 {
        self.rx_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The DO's endpoint_id, received from the Hello message on connect.
    /// Returns `None` if the Hello message has not yet been received.
    pub async fn do_endpoint_id(&self) -> Option<String> {
        self.do_endpoint_id.lock().await.clone()
    }

    async fn record_hello(
        &self,
        hello: proto::RelayHello,
        connection_manager: &Option<Arc<ConnectionManager>>,
    ) -> anyhow::Result<()> {
        if hello.auth_challenge.len() != 32 || hello.festival_id.is_empty() {
            anyhow::bail!("relay hello contains an invalid auth challenge");
        }
        *self.auth_challenge.lock().await = Some(RelayAuthChallenge {
            bytes: hello.auth_challenge,
            festival_id: hello.festival_id,
            expires_at: hello.challenge_expires_at,
        });
        *self.do_endpoint_id.lock().await = Some(hello.endpoint_id.clone());

        if let Some(cm) = connection_manager {
            let now_secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            cm.on_peer_list_updated(vec![crate::types::PeerInfo {
                endpoint_id: hello.endpoint_id,
                relay_url: None,
                last_seen: now_secs,
                user_id: "festival-do".to_string(),
            }]);
        }
        Ok(())
    }

    /// Send a gossip envelope to the DO on the given topic.
    pub async fn send_gossip(
        &self,
        topic: &str,
        envelope: &proto::GossipEnvelope,
    ) -> anyhow::Result<()> {
        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Gossip(
                proto::GossipRelay {
                    topic: topic.to_string(),
                    message: Some(envelope.clone()),
                },
            )),
        })
        .await
    }

    /// Send a gossip envelope and wait until the relay echoes its persisted
    /// sequence number back to this client. A socket write alone is not an
    /// acknowledgement: authentication or persistence may still fail.
    pub async fn send_gossip_confirmed(
        &self,
        topic: &str,
        envelope: &proto::GossipEnvelope,
    ) -> anyhow::Result<()> {
        if !self.is_connected() || !self.is_authenticated() {
            anyhow::bail!("relay is not authenticated");
        }

        let key = gossip_acknowledgement_key(topic, envelope);
        let (sender, receiver) = oneshot::channel();
        self.acknowledgement_waiters
            .lock()
            .await
            .entry(key)
            .or_default()
            .push(sender);

        if let Err(error) = self.send_gossip(topic, envelope).await {
            self.acknowledgement_waiters.lock().await.remove(&key);
            return Err(error);
        }

        match tokio::time::timeout(std::time::Duration::from_secs(10), receiver).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => anyhow::bail!("relay acknowledgement channel closed"),
            Err(_) => {
                self.acknowledgement_waiters.lock().await.remove(&key);
                anyhow::bail!("relay persistence acknowledgement timed out")
            }
        }
    }

    async fn acknowledge_gossip(&self, topic: &str, envelope: &proto::GossipEnvelope) -> bool {
        let key = gossip_acknowledgement_key(topic, envelope);
        let Some(waiters) = self.acknowledgement_waiters.lock().await.remove(&key) else {
            return false;
        };
        for waiter in waiters {
            let _ = waiter.send(());
        }
        true
    }

    /// Advertise support for persistence acknowledgements without changing the
    /// gossip wire envelope. Legacy relay clients never receive sender echoes.
    pub async fn enable_persistence_acknowledgements(&self) -> anyhow::Result<()> {
        self.subscribe(vec![RELAY_ACK_CAPABILITY_TOPIC.to_string()])
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
        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Subscribe(
                proto::SubscribeRequest { topics },
            )),
        })
        .await
    }

    /// Unsubscribe from topics on the DO.
    pub async fn unsubscribe(&self, topics: Vec<String>) -> anyhow::Result<()> {
        {
            let mut subs = self.subscribed_topics.lock().await;
            for t in &topics {
                subs.remove(t);
            }
        }
        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Unsubscribe(
                proto::UnsubscribeRequest { topics },
            )),
        })
        .await
    }

    /// Request catchup for a topic since a given sequence number.
    pub async fn request_catchup(&self, topic: &str, since_seq: u64) -> anyhow::Result<()> {
        if is_public_chat_topic(topic) {
            anyhow::bail!("public chat requires committed-head catch-up");
        }
        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Catchup(
                proto::CatchupRequest {
                    topic: topic.to_string(),
                    since_seq,
                },
            )),
        })
        .await
    }

    /// Send a state vector exchange request to the DO for a CRDT doc.
    pub async fn sv_exchange(&self, doc_id: &str, sv: &[u8]) -> anyhow::Result<()> {
        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::SvExchange(
                proto::SvExchangeRequest {
                    doc_id: doc_id.to_string(),
                    sv: sv.to_vec(),
                },
            )),
        })
        .await
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

    async fn send_client_msg(&self, msg: &proto::RelayClientMessage) -> anyhow::Result<()> {
        let bytes = msg.encode_to_vec();
        let len = bytes.len() as u64;
        self.sink
            .lock()
            .await
            .send(tungstenite::Message::Binary(bytes.into()))
            .await
            .map_err(|e| anyhow::anyhow!("ws send error: {e}"))?;
        self.tx_bytes
            .fetch_add(len, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Request chat messages since the given state vector.
    pub async fn chat_catchup(
        &self,
        topic: &str,
        sv: &crate::sync::ChatStateVector,
        limit: u32,
        encrypted_group_proof: &[u8],
    ) -> anyhow::Result<()> {
        // The relay is a blind mailbox for private groups and cannot filter
        // ciphertext by writer heads. Do not expose decrypted writer IDs to it.
        let (sequences, head_ids) = if topic.starts_with("group/") {
            (Default::default(), Default::default())
        } else {
            (sv.sequences(), sv.head_ids())
        };
        self.send_client_msg(&proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::ChatCatchup(
                proto::ChatCatchupRequest {
                    topic: topic.to_string(),
                    sv: sequences,
                    limit,
                    head_ids,
                    encrypted_group_proof: encrypted_group_proof.to_vec(),
                },
            )),
        })
        .await
    }

    /// Broadcast raw protobuf data on a topic (wraps in gossip envelope).
    pub async fn broadcast(&self, topic: &str, data: &[u8]) -> anyhow::Result<()> {
        let envelope = proto::decode_envelope(data)?;
        self.send_gossip(topic, &envelope).await
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
        if doc_id.starts_with("group/") {
            // The DO is an opaque mailbox and cannot compute a Yrs diff for
            // encrypted group state. Subscription replay supplies encrypted
            // group updates; never expose the plaintext state vector to it.
            return Ok(());
        }
        WsRelaySink::sv_exchange(self, doc_id, sv).await
    }

    async fn chat_catchup(
        &self,
        topic: &str,
        sv: &crate::sync::ChatStateVector,
        limit: u32,
        encrypted_group_proof: &[u8],
    ) -> anyhow::Result<Vec<crate::proto::GossipEnvelope>> {
        // The relay answers asynchronously with a `ChatDiff` server message that
        // the receive loop dispatches; nothing to return inline.
        WsRelaySink::chat_catchup(self, topic, sv, limit, encrypted_group_proof).await?;
        Ok(vec![])
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
/// If a `ConnectionManager` is provided, the DO's endpoint_id (received via
/// the Hello message) will be registered as a known peer.
pub async fn connect(
    url: &str,
    sync_orchestrator: Arc<SyncOrchestrator>,
    doc_manager: Arc<DocManager>,
    notifier: Arc<crate::notifier::ResourceNotifier>,
    connection_manager: Option<Arc<ConnectionManager>>,
) -> anyhow::Result<(
    WsRelaySink,
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>,
)> {
    tracing::info!("ws_relay: connecting to {url}");
    let (mut ws_stream, _response) =
        tokio::time::timeout(std::time::Duration::from_secs(10), connect_async(url))
            .await
            .map_err(|_| anyhow::anyhow!("ws connect timed out after 10s"))??;
    let hello = read_relay_hello(&mut ws_stream).await?;
    tracing::info!("ws_relay: connected, splitting stream");
    let (sink, stream) = ws_stream.split();

    let relay_sink = WsRelaySink {
        sink: Arc::new(Mutex::new(sink)),
        subscribed_topics: Arc::new(Mutex::new(HashSet::new())),
        last_seen_seq: Arc::new(Mutex::new(HashMap::new())),
        authenticated: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        reconnect_auth: Arc::new(Mutex::new(None)),
        acknowledgement_waiters: Arc::new(Mutex::new(HashMap::new())),
        shutdown: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        connected: Arc::new(std::sync::atomic::AtomicBool::new(true)),
        tx_bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        rx_bytes: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        do_endpoint_id: Arc::new(Mutex::new(None)),
        auth_challenge: Arc::new(Mutex::new(None)),
    };
    relay_sink.record_hello(hello, &connection_manager).await?;

    let recv_sink = relay_sink.clone();
    let recv_url = url.to_string();
    let receive_loop = Box::pin(run_receive_loop_with_reconnect(
        recv_sink,
        stream,
        recv_url,
        sync_orchestrator,
        doc_manager,
        notifier,
        connection_manager,
    ));

    Ok((relay_sink, receive_loop))
}

/// Connect with exponential backoff + full jitter, capped at 30s.
pub async fn connect_with_retry(
    url: &str,
    max_retries: u32,
    sync_orchestrator: Arc<SyncOrchestrator>,
    doc_manager: Arc<DocManager>,
    notifier: Arc<crate::notifier::ResourceNotifier>,
    connection_manager: Option<Arc<ConnectionManager>>,
) -> anyhow::Result<(
    WsRelaySink,
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>,
)> {
    use rand::RngExt;
    const MAX_DELAY_MS: u64 = 30_000;

    for attempt in 0..max_retries {
        match connect(
            url,
            sync_orchestrator.clone(),
            doc_manager.clone(),
            notifier.clone(),
            connection_manager.clone(),
        )
        .await
        {
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
    sync_orchestrator: Arc<SyncOrchestrator>,
    doc_manager: Arc<DocManager>,
    notifier: Arc<crate::notifier::ResourceNotifier>,
    connection_manager: Option<Arc<ConnectionManager>>,
) -> anyhow::Result<()> {
    use rand::RngExt;
    const MAX_DELAY_MS: u64 = 30_000;

    let mut stream = initial_stream;
    let mut reconnect_attempt: u32 = 0;

    loop {
        if sink.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let result = tokio::select! {
            result = run_receive_loop(
                &sink,
                &mut stream,
                &sync_orchestrator,
                &doc_manager,
                &notifier,
                &connection_manager,
            ) => result,
            () = sink.shutdown_notify.notified() => return Ok(()),
        };

        sink.connected
            .store(false, std::sync::atomic::Ordering::Relaxed);

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
            if sink.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }
            let base_ms = 1000u64.saturating_mul(1u64 << reconnect_attempt.min(5));
            let capped = base_ms.min(MAX_DELAY_MS);
            let jitter = rand::rng().random_range(0..=capped);
            tracing::info!(
                "ws_relay: reconnecting in {jitter}ms (attempt {})",
                reconnect_attempt + 1,
            );
            tokio::select! {
                () = tokio::time::sleep(std::time::Duration::from_millis(jitter)) => {}
                () = sink.shutdown_notify.notified() => return Ok(()),
            }

            let connection = tokio::select! {
                result = tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    connect_async(&url),
                ) => result,
                () = sink.shutdown_notify.notified() => return Ok(()),
            };
            if sink.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(());
            }

            match connection {
                Ok(Ok((mut ws_stream, _))) => {
                    let hello = match read_relay_hello(&mut ws_stream).await {
                        Ok(hello) => hello,
                        Err(error) => {
                            tracing::warn!(%error, "ws_relay: reconnect hello failed");
                            reconnect_attempt = reconnect_attempt.saturating_add(1);
                            continue;
                        }
                    };
                    if let Err(error) = sink.record_hello(hello, &connection_manager).await {
                        tracing::warn!(%error, "ws_relay: reconnect challenge rejected");
                        reconnect_attempt = reconnect_attempt.saturating_add(1);
                        continue;
                    }
                    let (new_sink, new_stream) = ws_stream.split();
                    sink.authenticated
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    sink.swap_sink(new_sink).await;
                    stream = new_stream;
                    reconnect_attempt = 0;
                    sink.connected
                        .store(true, std::sync::atomic::Ordering::Relaxed);

                    // A replacement socket is a new server session. Authenticate
                    // it before re-subscribing or allowing queued writes.
                    if let Some(auth) = sink.reconnect_auth.lock().await.clone()
                        && let Err(error) = sink.send_auth(&auth).await
                    {
                        tracing::warn!(%error, "ws_relay: re-authentication failed");
                    }

                    // Re-subscribe to all previously subscribed topics
                    let topics: Vec<String> = sink
                        .subscribed_topics
                        .lock()
                        .await
                        .iter()
                        .cloned()
                        .collect();
                    if !topics.is_empty()
                        && let Err(e) = sink.subscribe(topics.clone()).await
                    {
                        tracing::warn!("ws_relay: re-subscribe failed: {e}");
                    }

                    // Public append logs reconnect through committed writer heads;
                    // sequence replay remains only for non-chat CRDT resources.
                    let seqs = sink.last_seen_seq.lock().await.clone();
                    for topic in topics {
                        let result = if is_public_chat_topic(&topic) {
                            match sync_orchestrator.chat_state_vector(&topic) {
                                Ok(sv) => sink.chat_catchup(&topic, &sv, 50, &[]).await,
                                Err(error) => Err(error),
                            }
                        } else if let Some(seq) = seqs.get(&topic) {
                            sink.request_catchup(&topic, *seq).await
                        } else {
                            Ok(())
                        };
                        if let Err(error) = result {
                            tracing::warn!(%error, "ws_relay: catchup request failed");
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
    sync_orchestrator: &Arc<SyncOrchestrator>,
    doc_manager: &Arc<DocManager>,
    notifier: &Arc<crate::notifier::ResourceNotifier>,
    connection_manager: &Option<Arc<ConnectionManager>>,
) -> anyhow::Result<()> {
    while let Some(msg) = stream.next().await {
        match msg {
            Ok(tungstenite::Message::Binary(data)) => {
                sink.rx_bytes
                    .fetch_add(data.len() as u64, std::sync::atomic::Ordering::Relaxed);
                tracing::debug!("ws_relay: recv {} binary bytes", data.len());
                match proto::decode_server_msg(&data) {
                    Ok(server_msg) => {
                        if handle_server_message(
                            server_msg,
                            sink,
                            sync_orchestrator,
                            doc_manager,
                            notifier,
                            connection_manager,
                        )
                        .await
                        .is_err()
                        {
                            tracing::warn!("ws_relay dispatch failed");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("ws_relay protobuf decode error: {e}");
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => {
                tracing::info!("ws_relay: server closed connection");
                break;
            }
            Ok(_) => {} // text / ping / pong — ignore
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
    msg: proto::RelayServerMessage,
    sink: &WsRelaySink,
    sync_orchestrator: &Arc<SyncOrchestrator>,
    _doc_manager: &Arc<DocManager>,
    _notifier: &Arc<crate::notifier::ResourceNotifier>,
    connection_manager: &Option<Arc<ConnectionManager>>,
) -> anyhow::Result<()> {
    use proto::relay_server_message::Msg;

    let Some(msg_inner) = msg.msg else {
        tracing::warn!("ws_relay: received empty server message");
        return Ok(());
    };

    match msg_inner {
        Msg::Hello(hello) => sink.record_hello(hello, connection_manager).await,

        Msg::AuthOk(auth_ok) => {
            sink.authenticated
                .store(auth_ok.authenticated, std::sync::atomic::Ordering::Relaxed);
            tracing::info!(
                "ws_relay: auth result: authenticated={}, admin_count={}",
                auth_ok.authenticated,
                auth_ok.admin_count,
            );
            Ok(())
        }

        Msg::Gossip(broadcast) => {
            // The relay echoes a sender's persisted envelope. Resolve durable
            // outbox waiters before dispatching the idempotent CRDT update.
            if let Some(ref envelope) = broadcast.message
                && sink.acknowledge_gossip(&broadcast.topic, envelope).await
            {
                return Ok(());
            }

            // Public chat uses committed-head catch-up, never relay sequence.
            if !is_public_chat_topic(&broadcast.topic) {
                let mut seqs = sink.last_seen_seq.lock().await;
                let entry = seqs.entry(broadcast.topic.clone()).or_insert(0);
                if broadcast.seq > *entry {
                    *entry = broadcast.seq;
                }
            }

            if let Some(ref envelope) = broadcast.message {
                sync_orchestrator
                    .handle_incoming_envelope(&broadcast.topic, envelope)
                    .await
            } else {
                Ok(())
            }
        }

        Msg::Catchup(catchup) => {
            if is_public_chat_topic(&catchup.topic) {
                anyhow::bail!("rejected sequence-based public chat catch-up");
            }
            for entry in catchup.messages {
                // Track seq
                {
                    let mut seqs = sink.last_seen_seq.lock().await;
                    let current = seqs.entry(catchup.topic.clone()).or_insert(0);
                    if entry.seq > *current {
                        *current = entry.seq;
                    }
                }

                if let Some(ref envelope) = entry.message
                    && sync_orchestrator
                        .handle_incoming_envelope(&catchup.topic, envelope)
                        .await
                        .is_err()
                {
                    tracing::warn!("ws_relay catchup dispatch failed");
                }
            }
            Ok(())
        }

        Msg::SvDiff(sv_diff) => {
            anyhow::bail!(
                "rejected unsigned sv_diff for {}; festival catch-up requires a signed checkpoint",
                sv_diff.doc_id
            )
        }

        Msg::ChatDiff(chat_diff) => {
            sync_orchestrator.handle_incoming_chat_batch(&chat_diff.topic, &chat_diff.messages)?;
            tracing::info!("ws_relay: applied chat_diff");
            Ok(())
        }

        Msg::Subscribed(subscribed) => {
            tracing::info!(
                "ws_relay: active subscription count={}",
                subscribed.topics.len()
            );
            Ok(())
        }

        Msg::Error(err) => {
            tracing::warn!(
                "ws_relay: server error: {} (code {:?})",
                err.error,
                err.code
            );
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn gossip_acknowledgement_key(topic: &str, envelope: &proto::GossipEnvelope) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(topic.as_bytes());
    hasher.update(&[0]);
    hasher.update(&envelope.encode_to_vec());
    *hasher.finalize().as_bytes()
}

fn hex_to_bytes(hex: &str) -> anyhow::Result<Vec<u8>> {
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| anyhow::anyhow!("invalid hex: {e}"))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
                assert_eq!(
                    capped, MAX_DELAY_MS,
                    "should be capped at 30s by attempt {attempt}"
                );
            }
            prev_cap = capped;
        }
    }

    #[test]
    fn test_protobuf_subscribe_roundtrip() {
        let msg = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Subscribe(
                proto::SubscribeRequest {
                    topics: vec!["festival/test/chat".to_string()],
                },
            )),
        };
        let bytes = proto::encode_client_msg(&msg);
        let decoded = proto::decode_client_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_gossip_roundtrip() {
        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::Chat(proto::ChatMessage {
                id: "c1".to_string(),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: "hi".to_string(),
                topic: "t".to_string(),
                stage_id: None,
                timestamp: "now".to_string(),
                writer_seq: 0,
                logical_time: 0,
                writer_key: Vec::new(),
                signature: Vec::new(),
            })),
        };
        let msg = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Gossip(
                proto::GossipRelay {
                    topic: "festival/test/chat".to_string(),
                    message: Some(envelope.clone()),
                },
            )),
        };
        let bytes = proto::encode_client_msg(&msg);
        let decoded = proto::decode_client_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_catchup_roundtrip() {
        let msg = proto::RelayClientMessage {
            msg: Some(proto::relay_client_message::Msg::Catchup(
                proto::CatchupRequest {
                    topic: "festival/test/state".to_string(),
                    since_seq: 42,
                },
            )),
        };
        let bytes = proto::encode_client_msg(&msg);
        let decoded = proto::decode_client_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_server_gossip_roundtrip() {
        let msg = proto::RelayServerMessage {
            msg: Some(proto::relay_server_message::Msg::Gossip(
                proto::GossipBroadcast {
                    topic: "festival/test/chat".to_string(),
                    seq: 42,
                    message: Some(proto::GossipEnvelope {
                        payload: Some(proto::gossip_envelope::Payload::Chat(proto::ChatMessage {
                            id: "m1".to_string(),
                            user_id: "u1".to_string(),
                            display_name: "A".to_string(),
                            text: "t".to_string(),
                            topic: "t".to_string(),
                            stage_id: None,
                            timestamp: "now".to_string(),
                            writer_seq: 0,
                            logical_time: 0,
                            writer_key: Vec::new(),
                            signature: Vec::new(),
                        })),
                    }),
                },
            )),
        };
        let bytes = proto::encode_server_msg(&msg);
        let decoded = proto::decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_server_auth_ok_roundtrip() {
        let msg = proto::RelayServerMessage {
            msg: Some(proto::relay_server_message::Msg::AuthOk(proto::AuthOk {
                authenticated: true,
                admin_count: 1,
            })),
        };
        let bytes = proto::encode_server_msg(&msg);
        let decoded = proto::decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_server_error_roundtrip() {
        let msg = proto::RelayServerMessage {
            msg: Some(proto::relay_server_message::Msg::Error(proto::RelayError {
                error: "bad request".to_string(),
                code: proto::ErrorCode::Malformed as i32,
            })),
        };
        let bytes = proto::encode_server_msg(&msg);
        let decoded = proto::decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_sv_diff_roundtrip() {
        let msg = proto::RelayServerMessage {
            msg: Some(proto::relay_server_message::Msg::SvDiff(
                proto::SvDiffResponse {
                    doc_id: "festival/test/state".to_string(),
                    diff: vec![1, 0, 0, 0],
                },
            )),
        };
        let bytes = proto::encode_server_msg(&msg);
        let decoded = proto::decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_protobuf_hello_roundtrip() {
        let msg = proto::RelayServerMessage {
            msg: Some(proto::relay_server_message::Msg::Hello(proto::RelayHello {
                endpoint_id: "a".repeat(64),
                auth_challenge: vec![7; 32],
                festival_id: "festival-1".to_string(),
                challenge_expires_at: 42,
            })),
        };
        let bytes = proto::encode_server_msg(&msg);
        let decoded = proto::decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_hex_to_bytes() {
        let result = hex_to_bytes("deadbeef").unwrap();
        assert_eq!(result, vec![0xde, 0xad, 0xbe, 0xef]);
    }
}
