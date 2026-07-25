//! Integration tests for offbeat-core against an ephemeral `wrangler dev` server.
//!
//! Each test spawns its own server instance with fresh state.
//!
//! Run:
//!   cargo test --test integration
//!
//! ## Protocol
//!
//! The DO speaks binary protobuf over WebSocket:
//!
//! **Client→DO:** `RelayClientMessage` (auth, subscribe, gossip, catchup, etc.)
//! **DO→Client:** `RelayServerMessage` (auth_ok, subscribed, gossip, catchup, error)

mod harness;

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use harness::DevServer;
use offbeat_core::proto::{self, relay_client_message, relay_server_message};
use prost::Message as ProstMessage;
use serde_json::{Value, json};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;

/// Convert bytes to hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Convert hex string to bytes
fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

async fn register_festival_admin(
    server: &DevServer,
    festival_id: &str,
) -> ed25519_dalek::SigningKey {
    let key = offbeat_core::signing::generate_signing_key();
    let public_key = bytes_to_hex(&key.verifying_key().to_bytes());
    let client = reqwest::Client::new();
    client
        .put(format!(
            "{}/festivals/{festival_id}/config",
            server.http_url()
        ))
        .json(&json!({
            "festivalId": festival_id,
            "opensAt": "2020-01-01T00:00:00.000Z",
            "closesAt": "2030-12-31T23:59:59.999Z"
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    client
        .put(format!(
            "{}/festivals/{festival_id}/admins",
            server.http_url()
        ))
        .json(&json!({ "publicKey": public_key }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    key
}

async fn publish_festival_update(
    server: &DevServer,
    festival_id: &str,
    admin_key: &ed25519_dalek::SigningKey,
    update: &[u8],
) -> Value {
    use base64::Engine as _;

    let topic = format!("festival/{festival_id}/state");
    let auth_signature =
        offbeat_core::signing::sign(admin_key, format!("sign-update:{topic}").as_bytes());
    reqwest::Client::new()
        .post(format!(
            "{}/festivals/{festival_id}/sign-update",
            server.http_url()
        ))
        .json(&json!({
            "publicKey": bytes_to_hex(&admin_key.verifying_key().to_bytes()),
            "signature": bytes_to_hex(&auth_signature),
            "docId": topic,
            "topic": topic,
            "update": base64::engine::general_purpose::STANDARD.encode(update),
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

type WsSinkType = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsStreamType = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Connect to a festival's WebSocket endpoint (unauthenticated — can only read)
async fn connect_to_festival(
    ws_url: &str,
    festival_id: &str,
) -> Result<(WsSinkType, WsStreamType), anyhow::Error> {
    let url = format!("{ws_url}/festivals/{festival_id}/ws");
    let (ws_stream, _) = connect_async(&url).await?;
    Ok(ws_stream.split())
}

/// Send a binary protobuf RelayClientMessage over WebSocket.
async fn send_client_msg(
    sink: &mut WsSinkType,
    msg: proto::RelayClientMessage,
) -> Result<(), anyhow::Error> {
    let bytes = msg.encode_to_vec();
    sink.send(Message::Binary(bytes.into()))
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(())
}

/// Receive a binary protobuf RelayServerMessage with timeout.
async fn recv_server_msg(
    stream: &mut WsStreamType,
    timeout_secs: u64,
) -> Result<proto::RelayServerMessage, anyhow::Error> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("Timeout waiting for server message");
        }
        let msg = timeout(remaining, stream.next()).await?;
        match msg {
            Some(Ok(Message::Binary(data))) => {
                return proto::decode_server_msg(&data);
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) => anyhow::bail!("WebSocket closed"),
            Some(Ok(other)) => anyhow::bail!("Unexpected message type: {:?}", other),
            Some(Err(e)) => anyhow::bail!("WebSocket error: {}", e),
            None => anyhow::bail!("WebSocket closed unexpectedly"),
        }
    }
}

/// Wait for a specific server message variant. Returns the matched Msg.
async fn wait_for_msg(
    stream: &mut WsStreamType,
    matcher: impl Fn(&relay_server_message::Msg) -> bool,
    label: &str,
    timeout_secs: u64,
) -> Result<relay_server_message::Msg, anyhow::Error> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("Timeout waiting for {label}");
        }
        let server_msg = recv_server_msg(stream, remaining.as_secs().max(1)).await?;
        if let Some(ref msg) = server_msg.msg
            && matcher(msg)
        {
            return Ok(msg.clone());
        }
    }
}

/// Wait for an AuthOk message.
async fn wait_for_auth_ok(stream: &mut WsStreamType) -> Result<proto::AuthOk, anyhow::Error> {
    let msg = wait_for_msg(
        stream,
        |m| matches!(m, relay_server_message::Msg::AuthOk(_)),
        "auth_ok",
        5,
    )
    .await?;
    match msg {
        relay_server_message::Msg::AuthOk(ok) => Ok(ok),
        _ => unreachable!(),
    }
}

/// Wait for a Subscribed message.
async fn wait_for_subscribed(
    stream: &mut WsStreamType,
) -> Result<proto::Subscribed, anyhow::Error> {
    let msg = wait_for_msg(
        stream,
        |m| matches!(m, relay_server_message::Msg::Subscribed(_)),
        "subscribed",
        5,
    )
    .await?;
    match msg {
        relay_server_message::Msg::Subscribed(s) => Ok(s),
        _ => unreachable!(),
    }
}

/// Wait for a GossipBroadcast message.
async fn wait_for_gossip(
    stream: &mut WsStreamType,
    timeout_secs: u64,
) -> Result<proto::GossipBroadcast, anyhow::Error> {
    let msg = wait_for_msg(
        stream,
        |m| matches!(m, relay_server_message::Msg::Gossip(_)),
        "gossip",
        timeout_secs,
    )
    .await?;
    match msg {
        relay_server_message::Msg::Gossip(g) => Ok(g),
        _ => unreachable!(),
    }
}

/// Wait for a CatchupResponse message.
async fn wait_for_catchup(
    stream: &mut WsStreamType,
) -> Result<proto::CatchupResponse, anyhow::Error> {
    let msg = wait_for_msg(
        stream,
        |m| matches!(m, relay_server_message::Msg::Catchup(_)),
        "catchup",
        5,
    )
    .await?;
    match msg {
        relay_server_message::Msg::Catchup(c) => Ok(c),
        _ => unreachable!(),
    }
}

/// Connect to a festival's WebSocket endpoint and authenticate (can read + write)
async fn connect_and_auth(
    server: &DevServer,
    festival_id: &str,
) -> Result<(WsSinkType, WsStreamType), anyhow::Error> {
    reqwest::Client::new()
        .put(format!(
            "{}/festivals/{festival_id}/config",
            server.http_url()
        ))
        .json(&json!({
            "festivalId": festival_id,
            "opensAt": "2020-01-01T00:00:00.000Z",
            "closesAt": "2030-12-31T23:59:59.999Z"
        }))
        .send()
        .await?
        .error_for_status()?;

    let signing_key = {
        let mut seed = [0u8; 32];
        getrandom::getrandom(&mut seed).unwrap();
        ed25519_dalek::SigningKey::from_bytes(&seed)
    };
    let pubkey_hex = bytes_to_hex(&signing_key.verifying_key().to_bytes());

    // Register and get attestation (HTTP/JSON)
    let attestation_json = register_and_get_attestation(&server.http_url(), &pubkey_hex).await?;

    let (mut sink, mut stream) = connect_to_festival(&server.ws_url(), festival_id).await?;

    // Build protobuf auth request
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs()
        .to_string();
    let session_msg = format!("session:{timestamp}");
    use ed25519_dalek::Signer;
    let sig = signing_key.sign(session_msg.as_bytes());

    let att_msg = attestation_json["message"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let att_sig = hex_to_bytes(attestation_json["signature"].as_str().unwrap_or(""));
    let att_issuer = hex_to_bytes(attestation_json["issuer"].as_str().unwrap_or(""));

    send_client_msg(
        &mut sink,
        proto::RelayClientMessage {
            msg: Some(relay_client_message::Msg::Auth(proto::AuthRequest {
                public_key: signing_key.verifying_key().to_bytes().to_vec(),
                attestation: Some(proto::Attestation {
                    message: att_msg,
                    signature: att_sig,
                    issuer: att_issuer,
                }),
                signature: sig.to_bytes().to_vec(),
                timestamp,
            })),
        },
    )
    .await?;
    wait_for_auth_ok(&mut stream).await?;

    Ok((sink, stream))
}

// For timestamp generation in tests
mod chrono {
    pub struct Utc;
    impl Utc {
        pub fn now() -> Self {
            Utc
        }
        pub fn to_rfc3339(&self) -> String {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            format!("{}Z", now)
        }
    }
}

/// Build a GossipEnvelope for a chat message
fn chat_envelope(
    id: &str,
    user_id: &str,
    display_name: &str,
    text: &str,
    topic: &str,
) -> proto::GossipEnvelope {
    proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::Chat(proto::ChatMessage {
            id: id.to_string(),
            user_id: user_id.to_string(),
            display_name: display_name.to_string(),
            text: text.to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            writer_seq: 0,
        })),
    }
}

/// Send a gossip envelope on a topic.
async fn send_gossip(
    sink: &mut WsSinkType,
    topic: &str,
    envelope: proto::GossipEnvelope,
) -> Result<(), anyhow::Error> {
    send_client_msg(
        sink,
        proto::RelayClientMessage {
            msg: Some(relay_client_message::Msg::Gossip(proto::GossipRelay {
                topic: topic.to_string(),
                message: Some(envelope),
            })),
        },
    )
    .await
}

/// Send a subscribe request.
async fn send_subscribe(sink: &mut WsSinkType, topics: &[&str]) -> Result<(), anyhow::Error> {
    send_client_msg(
        sink,
        proto::RelayClientMessage {
            msg: Some(relay_client_message::Msg::Subscribe(
                proto::SubscribeRequest {
                    topics: topics.iter().map(|t| t.to_string()).collect(),
                },
            )),
        },
    )
    .await
}

/// Send a catchup request.
async fn send_catchup(
    sink: &mut WsSinkType,
    topic: &str,
    since_seq: u64,
) -> Result<(), anyhow::Error> {
    send_client_msg(
        sink,
        proto::RelayClientMessage {
            msg: Some(relay_client_message::Msg::Catchup(proto::CatchupRequest {
                topic: topic.to_string(),
                since_seq,
            })),
        },
    )
    .await
}

async fn send_sv_exchange(
    sink: &mut WsSinkType,
    doc_id: &str,
    state_vector: Vec<u8>,
) -> Result<(), anyhow::Error> {
    send_client_msg(
        sink,
        proto::RelayClientMessage {
            msg: Some(relay_client_message::Msg::SvExchange(
                proto::SvExchangeRequest {
                    doc_id: doc_id.to_string(),
                    sv: state_vector,
                },
            )),
        },
    )
    .await
}

/// Register with the MainDO and get an attestation for the given Ed25519 keypair.
async fn register_and_get_attestation(
    http_url: &str,
    pubkey_hex: &str,
) -> Result<Value, anyhow::Error> {
    let client = reqwest::Client::new();
    // Register — get challenge from begin step
    let resp = client
        .post(format!("{http_url}/auth/register/begin"))
        .json(&json!({ "userId": pubkey_hex }))
        .send()
        .await?;
    let options: Value = resp.json().await?;
    let challenge = options["challenge"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("missing challenge in register/begin response"))?;

    // Complete registration with Ed25519 public key and challenge
    let resp = client
        .post(format!("{http_url}/auth/register/complete"))
        .json(&json!({
            "webauthnResponse": {},
            "challenge": challenge,
            "ed25519PublicKey": pubkey_hex
        }))
        .send()
        .await?;
    let body: Value = resp.json().await?;
    Ok(body["attestation"].clone())
}

/// Connect to a festival WebSocket, authenticate, subscribe to a topic, and
/// return the open sink/stream pair.
async fn connect_and_subscribe(
    server: &DevServer,
    festival_id: &str,
    topic: &str,
) -> Result<(WsSinkType, WsStreamType), anyhow::Error> {
    let (mut sink, mut stream) = connect_and_auth(server, festival_id).await?;
    send_subscribe(&mut sink, &[topic]).await?;
    wait_for_subscribed(&mut stream).await?;
    Ok((sink, stream))
}

// =============================================================================
// Test: Single client connects and subscribes
// =============================================================================

#[tokio::test]
async fn test_single_client_subscribe() {
    let server = DevServer::start().await;
    let (mut sink, mut stream) = connect_and_auth(&server, "rust-test-1").await.unwrap();

    send_subscribe(&mut sink, &["festival/rust-test-1/chat"])
        .await
        .unwrap();

    let response = wait_for_subscribed(&mut stream).await.unwrap();
    assert!(
        response
            .topics
            .iter()
            .any(|t| t == "festival/rust-test-1/chat")
    );
}

// =============================================================================
// Test: Single client sends gossip and retrieves via catchup
// =============================================================================

#[tokio::test]
async fn test_single_client_chat_and_catchup() {
    let server = DevServer::start().await;
    let (mut sink, mut stream) = connect_and_auth(&server, "rust-test-2").await.unwrap();

    send_subscribe(&mut sink, &["festival/rust-test-2/chat"])
        .await
        .unwrap();
    wait_for_subscribed(&mut stream).await.unwrap();

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = chat_envelope(
        &msg_id,
        "rust-user-1",
        "Rust User",
        "Hello from Rust!",
        "festival/rust-test-2/chat",
    );
    send_gossip(&mut sink, "festival/rust-test-2/chat", envelope)
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    send_catchup(&mut sink, "festival/rust-test-2/chat", 0)
        .await
        .unwrap();

    let catchup = wait_for_catchup(&mut stream).await.unwrap();
    assert_eq!(catchup.topic, "festival/rust-test-2/chat");

    assert!(!catchup.messages.is_empty());

    let our_msg = catchup
        .messages
        .iter()
        .find(|e| {
            matches!(
                e.message.as_ref().and_then(|m| m.payload.as_ref()),
                Some(proto::gossip_envelope::Payload::Chat(chat)) if chat.id == msg_id
            )
        })
        .expect("Our message should be in catchup");
    assert!(matches!(
        our_msg.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::Chat(_))
    ));
}

// =============================================================================
// Test: Two clients — D1 sends gossip, D2 receives via relay
// =============================================================================

#[tokio::test]
async fn test_two_clients_relay() {
    let server = DevServer::start().await;
    let topic = "festival/rust-test-3/chat";

    let (mut sink_a, _stream_a) = connect_and_subscribe(&server, "rust-test-3", topic)
        .await
        .unwrap();
    let (_sink_b, mut stream_b) = connect_and_subscribe(&server, "rust-test-3", topic)
        .await
        .unwrap();

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = chat_envelope(
        &msg_id,
        "client-a",
        "Client A",
        "Message from A to B via DO relay",
        topic,
    );
    send_gossip(&mut sink_a, topic, envelope).await.unwrap();

    let received = wait_for_gossip(&mut stream_b, 5).await.unwrap();
    assert_eq!(received.topic, topic);
    assert!(matches!(
        received.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::Chat(_))
    ));
    assert!(received.seq > 0);
}

// =============================================================================
// Test: P2P relay — GossipWireMessage with CRDT data
// =============================================================================

#[tokio::test]
async fn test_p2p_relay_update() {
    let server = DevServer::start().await;
    let topic = "group/test-group/state";

    let (mut sink_a, _stream_a) = connect_and_subscribe(&server, "rust-test-4", topic)
        .await
        .unwrap();
    let (_sink_b, mut stream_b) = connect_and_subscribe(&server, "rust-test-4", topic)
        .await
        .unwrap();

    let envelope = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::GroupUpdate(
            proto::GroupUpdate {
                doc_id: "test-doc".to_string(),
                encrypted: b"crdt-update-from-p2p-client".to_vec(),
                group_key_id: "test-key-id".to_string(),
            },
        )),
    };
    send_gossip(&mut sink_a, topic, envelope).await.unwrap();

    let received = wait_for_gossip(&mut stream_b, 5).await.unwrap();
    assert_eq!(received.topic, topic);
    assert!(matches!(
        received.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::GroupUpdate(_))
    ));
    assert!(received.seq > 0);
}

// =============================================================================
// Test: Yrs CRDT document sync via DO relay
// =============================================================================

#[tokio::test]
async fn test_yrs_crdt_sync_via_relay() {
    let server = DevServer::start().await;
    use yrs::{Doc, GetString, ReadTxn, Text, Transact};

    let topic = "festival/rust-test-5/state";

    let doc_a = Doc::new();
    let text_a = doc_a.get_or_insert_text("content");
    {
        let mut txn = doc_a.transact_mut();
        text_a.insert(&mut txn, 0, "Hello from client A");
    }

    let update_a = doc_a
        .transact()
        .encode_state_as_update_v1(&yrs::StateVector::default());

    let (mut sink_a, _stream_a) = connect_and_subscribe(&server, "rust-test-5", topic)
        .await
        .unwrap();
    let (_sink_b, mut stream_b) = connect_and_subscribe(&server, "rust-test-5", topic)
        .await
        .unwrap();

    let envelope = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::GroupUpdate(
            proto::GroupUpdate {
                doc_id: "test-crdt-doc".to_string(),
                encrypted: update_a.clone(),
                group_key_id: String::new(),
            },
        )),
    };
    send_gossip(&mut sink_a, topic, envelope).await.unwrap();

    let received = wait_for_gossip(&mut stream_b, 5).await.unwrap();
    let gu = match received.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::GroupUpdate(gu)) => gu,
        other => panic!("expected GroupUpdate, got {other:?}"),
    };

    let doc_b = Doc::new();
    let text_b = doc_b.get_or_insert_text("content");
    {
        let mut txn = doc_b.transact_mut();
        txn.apply_update(yrs::Update::decode_v1(&gu.encrypted).unwrap())
            .unwrap();
    }

    let content_b = {
        let txn = doc_b.transact();
        text_b.get_string(&txn)
    };
    assert_eq!(content_b, "Hello from client A");
}

// =============================================================================
// Test: Late joiner catches up via catchup mechanism
// =============================================================================

#[tokio::test]
async fn test_late_joiner_catchup() {
    let server = DevServer::start().await;
    let topic = "festival/rust-test-6/chat";

    let (mut sink_a, _stream_a) = connect_and_subscribe(&server, "rust-test-6", topic)
        .await
        .unwrap();

    let msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = chat_envelope(
        &msg_id,
        "early-user",
        "Early User",
        "Message sent before late joiner",
        topic,
    );
    send_gossip(&mut sink_a, topic, envelope).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let (mut sink_b, mut stream_b) = connect_and_subscribe(&server, "rust-test-6", topic)
        .await
        .unwrap();

    send_catchup(&mut sink_b, topic, 0).await.unwrap();

    let catchup = wait_for_catchup(&mut stream_b).await.unwrap();

    let our_msg = catchup
        .messages
        .iter()
        .find(|e| {
            matches!(
                e.message.as_ref().and_then(|m| m.payload.as_ref()),
                Some(proto::gossip_envelope::Payload::Chat(chat)) if chat.id == msg_id
            )
        })
        .expect("Missed message should be in catchup");
    assert!(matches!(
        our_msg.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::Chat(_))
    ));
}

// =============================================================================
// Test: D1 sends gossip → D2 receives; D2 sends → D1 receives
// =============================================================================

#[tokio::test]
async fn test_d1_d2_s1_relay_chat() {
    let server = DevServer::start().await;
    let topic = "festival/relay-test-1/chat";

    let (mut sink_d1, mut stream_d1) = connect_and_subscribe(&server, "relay-test-1", topic)
        .await
        .unwrap();
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(&server, "relay-test-1", topic)
        .await
        .unwrap();

    // D1 → D2
    let msg_id_1 = uuid::Uuid::new_v4().to_string();
    let envelope = chat_envelope(&msg_id_1, "d1", "D1", "Hello from D1", topic);
    send_gossip(&mut sink_d1, topic, envelope).await.unwrap();

    let recv_d2 = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    assert!(matches!(
        recv_d2.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::Chat(_))
    ));

    // D2 → D1
    let msg_id_2 = uuid::Uuid::new_v4().to_string();
    let envelope2 = chat_envelope(&msg_id_2, "d2", "D2", "Hello from D2", topic);
    send_gossip(&mut sink_d2, topic, envelope2).await.unwrap();

    let recv_d1 = wait_for_gossip(&mut stream_d1, 5).await.unwrap();
    assert!(matches!(
        recv_d1.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::Chat(_))
    ));
}

// =============================================================================
// Test: D1 sends encrypted CRDT via gossip → D2 receives and applies
// =============================================================================

#[tokio::test]
async fn test_d1_d2_s1_relay_crdt_update() {
    let server = DevServer::start().await;
    use offbeat_core::crypto;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let topic = "festival/relay-test-2/state";

    let group_key = crypto::generate_group_key();

    let doc_d1 = Doc::new();
    let map_d1 = doc_d1.get_or_insert_map("root");
    {
        let mut txn = doc_d1.transact_mut();
        map_d1.insert(&mut txn, "meetup", "main-stage");
    }
    let update_bytes = doc_d1
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let encrypted = crypto::encrypt(&group_key, &update_bytes).unwrap();

    let (mut sink_d1, _stream_d1) = connect_and_subscribe(&server, "relay-test-2", topic)
        .await
        .unwrap();
    let (_sink_d2, mut stream_d2) = connect_and_subscribe(&server, "relay-test-2", topic)
        .await
        .unwrap();

    let envelope = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::GroupUpdate(
            proto::GroupUpdate {
                doc_id: "group-doc".to_string(),
                encrypted: encrypted.clone(),
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };
    send_gossip(&mut sink_d1, topic, envelope).await.unwrap();

    let recv = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    let gu = match recv.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::GroupUpdate(gu)) => gu,
        other => panic!("expected GroupUpdate, got {other:?}"),
    };

    let decrypted = crypto::decrypt(&group_key, &gu.encrypted).unwrap();

    let doc_d2 = Doc::new();
    let map_d2 = doc_d2.get_or_insert_map("root");
    {
        let mut txn = doc_d2.transact_mut();
        txn.apply_update(yrs::Update::decode_v1(&decrypted).unwrap())
            .unwrap();
    }

    let val = {
        let txn = doc_d2.transact();
        match map_d2.get(&txn, "meetup") {
            Some(yrs::Out::Any(yrs::any::Any::String(s))) => Some(s.to_string()),
            _ => None,
        }
    };
    assert_eq!(val, Some("main-stage".to_string()));
}

// =============================================================================
// Test: D1 disconnects, D2 sends messages, D1 reconnects and catches up
// =============================================================================

#[tokio::test]
async fn test_d1_disconnect_d2_sends_d1_catchup() {
    let server = DevServer::start().await;
    let topic = "festival/relay-test-3/chat";

    let (mut sink_d1, stream_d1) = connect_and_subscribe(&server, "relay-test-3", topic)
        .await
        .unwrap();

    let initial_msg_id = uuid::Uuid::new_v4().to_string();
    let envelope = chat_envelope(&initial_msg_id, "d1", "D1", "D1 initial message", topic);
    send_gossip(&mut sink_d1, topic, envelope).await.unwrap();

    tokio::time::sleep(Duration::from_millis(100)).await;

    drop(sink_d1);
    drop(stream_d1);

    let (mut sink_d2, _stream_d2) = connect_and_subscribe(&server, "relay-test-3", topic)
        .await
        .unwrap();

    let mut d2_msg_ids = Vec::new();
    for i in 1..=3 {
        let msg_id = uuid::Uuid::new_v4().to_string();
        d2_msg_ids.push(msg_id.clone());
        let envelope = chat_envelope(&msg_id, "d2", "D2", &format!("D2 message {i}"), topic);
        send_gossip(&mut sink_d2, topic, envelope).await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (mut sink_d1_new, mut stream_d1_new) =
        connect_and_subscribe(&server, "relay-test-3", topic)
            .await
            .unwrap();

    send_catchup(&mut sink_d1_new, topic, 0).await.unwrap();

    let catchup = wait_for_catchup(&mut stream_d1_new).await.unwrap();

    // All messages should be present
    assert!(
        catchup.messages.len() >= 4,
        "should have at least 4 messages in catchup"
    );

    drop(sink_d1_new);
    drop(sink_d2);
}

// =============================================================================
// Test: Group encrypted state sync via DO relay (with SV handshake)
// =============================================================================

#[tokio::test]
async fn test_group_encrypted_state_sync_via_relay() {
    let server = DevServer::start().await;
    use offbeat_core::{OffbeatNode, crypto};

    let festival_id = "relay-group-test-1";

    let node_d1 = OffbeatNode::new_in_memory().unwrap();
    let create_result = node_d1
        .group_manager
        .create_group(festival_id, "Test Crew", "d1-user", "D1 User")
        .await
        .unwrap();

    let group_id = &create_result.group_id;
    let group_key = node_d1.db.load_group_key(group_id).unwrap().unwrap();
    let doc_id = format!("group/{group_id}/state");
    let topic = doc_id.clone();

    node_d1
        .group_manager
        .add_pin(group_id, "pin-relay-1", "Tent Area", "51.5,-0.1", "d1-user")
        .await
        .unwrap();

    let node_d2 = OffbeatNode::new_in_memory().unwrap();
    node_d2
        .group_manager
        .join_group(&create_result.invite_payload, "d2-user", "D2 User")
        .await
        .unwrap();

    let (mut sink_d1, mut stream_d1) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    // D2 sends SV as a gossip message
    let encrypted_sv_d2 = node_d2
        .group_manager
        .request_group_sync(group_id)
        .await
        .unwrap();
    let envelope_sv = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::SyncRequest(
            proto::SyncRequest {
                doc_id: doc_id.clone(),
                encrypted_sv: encrypted_sv_d2,
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };
    send_gossip(&mut sink_d2, &topic, envelope_sv)
        .await
        .unwrap();

    // D1 receives, computes diff, sends back
    let recv = wait_for_gossip(&mut stream_d1, 5).await.unwrap();
    let sr = match recv.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::SyncRequest(sr)) => sr,
        other => panic!("expected SyncRequest, got {other:?}"),
    };
    let diff_for_d2 = node_d1
        .group_manager
        .handle_sync_request(group_id, &sr.encrypted_sv)
        .await
        .unwrap();
    let envelope_diff = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::SyncResponse(
            proto::SyncResponse {
                doc_id: doc_id.clone(),
                encrypted_diff: diff_for_d2,
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };
    send_gossip(&mut sink_d1, &topic, envelope_diff)
        .await
        .unwrap();

    // D2 receives diff and applies
    let recv_d2 = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    let sr_resp = match recv_d2.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::SyncResponse(sr)) => sr,
        other => panic!("expected SyncResponse, got {other:?}"),
    };
    let diff_bytes = crypto::decrypt(&group_key, &sr_resp.encrypted_diff).unwrap();
    node_d2
        .doc_manager
        .apply_update(&doc_id, &diff_bytes)
        .unwrap();

    let state_d2 = node_d2
        .group_manager
        .get_group_state(group_id)
        .await
        .unwrap();
    assert_eq!(state_d2.pins.len(), 1);
    assert_eq!(state_d2.pins[0].label, "Tent Area");

    drop(sink_d1);
    drop(sink_d2);
}

// =============================================================================
// Test: Full SV handshake via DO relay
// =============================================================================

#[tokio::test]
async fn test_sv_handshake_group_sync() {
    let server = DevServer::start().await;
    use offbeat_core::{OffbeatNode, crypto};

    let festival_id = "sv-handshake-test-1";

    let node_d1 = OffbeatNode::new_in_memory().unwrap();
    let create_result = node_d1
        .group_manager
        .create_group(festival_id, "SV Crew", "d1-user", "D1 User")
        .await
        .unwrap();

    let group_id = &create_result.group_id;
    let group_key = node_d1.db.load_group_key(group_id).unwrap().unwrap();
    let doc_id = format!("group/{group_id}/state");
    let topic = doc_id.clone();

    node_d1
        .group_manager
        .add_pin(group_id, "pin-sv-1", "Base Camp", "52.0,0.1", "d1-user")
        .await
        .unwrap();
    node_d1
        .group_manager
        .check_in(group_id, "d1-user", Some("main-stage"), None)
        .await
        .unwrap();

    let node_d2 = OffbeatNode::new_in_memory().unwrap();
    node_d2
        .group_manager
        .join_group(&create_result.invite_payload, "d2-user", "D2 User")
        .await
        .unwrap();

    let (mut sink_d1, mut stream_d1) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    // D2 → sync_request → D1
    let encrypted_sv = node_d2
        .group_manager
        .request_group_sync(group_id)
        .await
        .unwrap();
    let envelope_req = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::SyncRequest(
            proto::SyncRequest {
                doc_id: doc_id.clone(),
                encrypted_sv,
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };
    send_gossip(&mut sink_d2, &topic, envelope_req)
        .await
        .unwrap();

    // D1 receives, responds
    let recv = wait_for_gossip(&mut stream_d1, 5).await.unwrap();
    let sr = match recv.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::SyncRequest(sr)) => sr,
        other => panic!("expected SyncRequest, got {other:?}"),
    };
    let diff_for_d2 = node_d1
        .group_manager
        .handle_sync_request(group_id, &sr.encrypted_sv)
        .await
        .unwrap();
    let envelope_resp = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::SyncResponse(
            proto::SyncResponse {
                doc_id: doc_id.clone(),
                encrypted_diff: diff_for_d2,
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };
    send_gossip(&mut sink_d1, &topic, envelope_resp)
        .await
        .unwrap();

    // D2 applies
    let recv_d2 = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    let sr_resp = match recv_d2.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::SyncResponse(sr)) => sr,
        other => panic!("expected SyncResponse, got {other:?}"),
    };
    let diff = crypto::decrypt(&group_key, &sr_resp.encrypted_diff).unwrap();
    node_d2.doc_manager.apply_update(&doc_id, &diff).unwrap();

    let state_d2 = node_d2
        .group_manager
        .get_group_state(group_id)
        .await
        .unwrap();
    assert_eq!(state_d2.pins.len(), 1);
    assert_eq!(state_d2.pins[0].label, "Base Camp");
    let d1_member = state_d2
        .members
        .iter()
        .find(|m| m.user_id == "d1-user")
        .expect("d1-user");
    assert_eq!(d1_member.stage_id.as_deref(), Some("main-stage"));

    drop(sink_d1);
    drop(sink_d2);
}

// =============================================================================
// Test: Festival stage chat — D1 and D2 on stage-1, D1 sends on stage-2
//       where D2 is NOT subscribed → D2 does NOT receive it
// =============================================================================

#[tokio::test]
async fn test_festival_stage_chat_via_relay() {
    let server = DevServer::start().await;
    let festival_id = "chat-stage-test-1";
    let topic_stage1 = format!("festival/{festival_id}/chat/stage-1");
    let topic_stage2 = format!("festival/{festival_id}/chat/stage-2");

    let (mut sink_d1, _stream_d1) = connect_and_subscribe(&server, festival_id, &topic_stage1)
        .await
        .unwrap();
    let (_sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic_stage1)
        .await
        .unwrap();

    // Also subscribe D1 to stage-2
    send_subscribe(&mut sink_d1, &[&topic_stage2])
        .await
        .unwrap();

    // D1 sends on stage-1 → D2 receives
    let msg_id_1 = uuid::Uuid::new_v4().to_string();
    let envelope1 = chat_envelope(&msg_id_1, "d1", "D1", "Stage 1 message", &topic_stage1);
    send_gossip(&mut sink_d1, &topic_stage1, envelope1)
        .await
        .unwrap();

    let recv = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    assert!(matches!(
        recv.message.as_ref().and_then(|m| m.payload.as_ref()),
        Some(proto::gossip_envelope::Payload::Chat(_))
    ));

    // D1 sends on stage-2 → D2 should NOT receive (timeout)
    let msg_id_2 = uuid::Uuid::new_v4().to_string();
    let envelope2 = chat_envelope(&msg_id_2, "d1", "D1", "Stage 2 message", &topic_stage2);
    send_gossip(&mut sink_d1, &topic_stage2, envelope2)
        .await
        .unwrap();

    let result =
        tokio::time::timeout(Duration::from_secs(2), wait_for_gossip(&mut stream_d2, 2)).await;
    match result {
        Err(_) => {}       // expected timeout
        Ok(Ok(_msg)) => {} // could be another message, acceptable
        Ok(Err(_)) => {}   // connection error — acceptable
    }

    drop(sink_d1);
    drop(_sink_d2);
}

// =============================================================================
// Test: Encrypted group chat via relay
// =============================================================================

#[tokio::test]
async fn test_encrypted_group_chat_via_relay() {
    let server = DevServer::start().await;
    use offbeat_core::crypto;
    use offbeat_core::types::ChatMessage;

    let festival_id = "chat-group-test-1";
    let group_key = crypto::generate_group_key();
    let group_id = crypto::group_id_from_key(&group_key);
    let topic = format!("group/{group_id}/chat");

    let original = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: "d1-user".to_string(),
        display_name: "D1".to_string(),
        text: "secret group hello".to_string(),
        topic: topic.clone(),
        stage_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        writer_seq: 0,
    };
    let plaintext = serde_json::to_vec(&original).unwrap();
    let encrypted = crypto::encrypt(&group_key, &plaintext).unwrap();

    let envelope = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::EncryptedChat(
            proto::EncryptedPayload {
                encrypted: encrypted.clone(),
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };

    let (mut sink_d1, _) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    let (_sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    send_gossip(&mut sink_d1, &topic, envelope).await.unwrap();

    let recv_d2 = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    let ec = match recv_d2.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::EncryptedChat(ec)) => ec,
        other => panic!("expected EncryptedChat, got {other:?}"),
    };

    // D2 can decrypt
    let pt = crypto::decrypt(&group_key, &ec.encrypted).unwrap();
    let msg: ChatMessage = serde_json::from_slice(&pt).unwrap();
    assert_eq!(msg.text, "secret group hello");

    // Wrong key fails
    let wrong_key = crypto::generate_group_key();
    let decrypt_result = crypto::decrypt(&wrong_key, &ec.encrypted);
    assert!(decrypt_result.is_err());

    drop(sink_d1);
}

// =============================================================================
// Test: Chat catch-up on reconnect
// =============================================================================

#[tokio::test]
async fn test_chat_catchup_on_reconnect() {
    let server = DevServer::start().await;
    let festival_id = "chat-catchup-test-1";
    let topic = format!("festival/{festival_id}/chat/general");

    let (mut sink_d1, _stream_d1) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    let mut msg_ids = Vec::new();
    for i in 1..=5 {
        let msg_id = uuid::Uuid::new_v4().to_string();
        msg_ids.push(msg_id.clone());
        let envelope = chat_envelope(&msg_id, "d1", "D1", &format!("Catchup message {i}"), &topic);
        send_gossip(&mut sink_d1, &topic, envelope).await.unwrap();
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    send_catchup(&mut sink_d2, &topic, 0).await.unwrap();

    let catchup = wait_for_catchup(&mut stream_d2).await.unwrap();

    assert!(
        catchup.messages.len() >= 5,
        "should have at least 5 messages in catchup"
    );

    drop(sink_d1);
    drop(sink_d2);
}

// =============================================================================
// Test: Festival stage chat — multiple stages, messages routed correctly
// =============================================================================

#[tokio::test]
async fn test_festival_stage_chat_multi_stage_routing() {
    let server = DevServer::start().await;
    let festival_id = "stage-routing-test-1";
    let topic_main = format!("festival/{festival_id}/chat/main-stage");
    let topic_second = format!("festival/{festival_id}/chat/second-stage");
    let topic_general = format!("festival/{festival_id}/chat/general");

    // D1 subscribes to all three topics
    let (mut sink_d1, mut stream_d1) = connect_and_auth(&server, festival_id).await.unwrap();
    send_subscribe(&mut sink_d1, &[&topic_main, &topic_second, &topic_general])
        .await
        .unwrap();
    wait_for_subscribed(&mut stream_d1).await.unwrap();

    // D2 subscribes only to main-stage
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic_main)
        .await
        .unwrap();

    // D3 subscribes only to general
    let (_sink_d3, mut stream_d3) = connect_and_subscribe(&server, festival_id, &topic_general)
        .await
        .unwrap();

    // D1 sends on main-stage → D2 receives, D3 does not
    let msg_id_main = uuid::Uuid::new_v4().to_string();
    let envelope_main = chat_envelope(&msg_id_main, "d1", "D1", "Main stage rocks!", &topic_main);
    send_gossip(&mut sink_d1, &topic_main, envelope_main)
        .await
        .unwrap();

    let recv_d2 = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
    match recv_d2.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::Chat(chat)) => {
            assert_eq!(chat.text, "Main stage rocks!");
        }
        other => panic!("expected Chat, got {other:?}"),
    }

    // D3 should not receive (timeout)
    let d3_result = wait_for_gossip(&mut stream_d3, 1).await;
    assert!(
        d3_result.is_err(),
        "D3 should not receive main-stage messages"
    );

    // D1 sends on general → D3 receives, D2 does not
    let msg_id_gen = uuid::Uuid::new_v4().to_string();
    let envelope_gen = chat_envelope(
        &msg_id_gen,
        "d1",
        "D1",
        "General announcement",
        &topic_general,
    );
    send_gossip(&mut sink_d1, &topic_general, envelope_gen)
        .await
        .unwrap();

    let recv_d3 = wait_for_gossip(&mut stream_d3, 5).await.unwrap();
    match recv_d3.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::Chat(chat)) => {
            assert_eq!(chat.text, "General announcement");
        }
        other => panic!("expected Chat, got {other:?}"),
    }

    // D2 sends reply on main-stage → D1 receives
    let msg_id_reply = uuid::Uuid::new_v4().to_string();
    let envelope_reply = chat_envelope(&msg_id_reply, "d2", "D2", "Agreed!", &topic_main);
    send_gossip(&mut sink_d2, &topic_main, envelope_reply)
        .await
        .unwrap();

    let recv_d1 = wait_for_gossip(&mut stream_d1, 5).await.unwrap();
    match recv_d1.message.as_ref().and_then(|m| m.payload.as_ref()) {
        Some(proto::gossip_envelope::Payload::Chat(chat)) => {
            assert_eq!(chat.text, "Agreed!");
        }
        other => panic!("expected Chat, got {other:?}"),
    }
}

// =============================================================================
// Test: Signed festival_update propagates N1 → DO → N2, N2 verifies signature
// =============================================================================

#[tokio::test]
async fn test_signed_festival_update_propagation() {
    use base64::Engine as _;
    use offbeat_core::signing;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let server = DevServer::start().await;
    let festival_id = "signed-prop-test-1";
    let topic = format!("festival/{festival_id}/state");
    let base_url = server.http_url();
    let client = reqwest::Client::new();

    let admin_key = signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());
    let (_subscriber, mut live_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    client
        .put(format!("{base_url}/festivals/{festival_id}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let source = Doc::new();
    let map = source.get_or_insert_map("root");
    {
        let mut txn = source.transact_mut();
        map.insert(&mut txn, "headliner", "Aphex Twin");
        map.insert(&mut txn, "stage", "main-stage");
    }
    let update = source
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let admin_sig = signing::sign(&admin_key, format!("sign-update:{topic}").as_bytes());
    let response = client
        .post(format!("{base_url}/festivals/{festival_id}/sign-update"))
        .json(&json!({
            "publicKey": admin_pub_hex,
            "signature": bytes_to_hex(&admin_sig),
            "docId": topic,
            "topic": topic,
            "update": base64::engine::general_purpose::STANDARD.encode(update),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let response: Value = response.json().await.unwrap();
    let public_key: [u8; 32] = hex_to_bytes(response["publicKey"].as_str().unwrap())
        .try_into()
        .unwrap();

    let live = wait_for_gossip(&mut live_stream, 5).await.unwrap();
    let live_update = match live.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed festival delta, got {other:?}"),
    };
    assert_eq!(live_update.kind, 1);
    let signed = live_update.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &public_key,
        &live_update.doc_id,
        live_update.kind,
        live_update.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let (mut late_sink, mut late_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_sv_exchange(
        &mut late_sink,
        &topic,
        yrs::StateVector::default().encode_v1(),
    )
    .await
    .unwrap();
    let checkpoint = wait_for_gossip(&mut late_stream, 5).await.unwrap();
    let checkpoint = match checkpoint.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed festival checkpoint, got {other:?}"),
    };
    assert_eq!(checkpoint.kind, 2);
    let signed = checkpoint.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &public_key,
        &checkpoint.doc_id,
        checkpoint.kind,
        checkpoint.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let late_doc = Doc::new();
    let late_map = late_doc.get_or_insert_map("root");
    late_doc
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(&signed.update).unwrap())
        .unwrap();
    let txn = late_doc.transact();
    match late_map.get(&txn, "headliner") {
        Some(yrs::Out::Any(yrs::any::Any::String(value))) => {
            assert_eq!(value.to_string(), "Aphex Twin")
        }
        other => panic!("expected checkpoint lineup, got {other:?}"),
    }
}

// =============================================================================
// Test: Signed update rejected by wrong key — trusted relay is not blindly trusted
// =============================================================================

#[tokio::test]
async fn test_signed_update_rejected_by_wrong_key() {
    let server = DevServer::start().await;
    let festival_id = "signed-reject-test-1";
    let topic = format!("festival/{festival_id}/state");
    let envelope = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::FestivalUpdate(
            proto::FestivalUpdate {
                doc_id: topic.clone(),
                kind: 1,
                authority_seq: 1,
                signed_update: Some(proto::SignedUpdate {
                    update: vec![0, 0],
                    author: "attacker".to_string(),
                    signature: vec![0; 64],
                }),
            },
        )),
    };

    let (mut sink, mut sender_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    let (_receiver, mut receiver_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    send_gossip(&mut sink, &topic, envelope).await.unwrap();
    let error = wait_for_msg(
        &mut sender_stream,
        |message| matches!(message, relay_server_message::Msg::Error(_)),
        "festival update rejection",
        5,
    )
    .await
    .unwrap();
    match error {
        relay_server_message::Msg::Error(error) => {
            assert!(error.error.contains("cannot send festival updates"));
        }
        _ => unreachable!(),
    }
    assert!(wait_for_gossip(&mut receiver_stream, 1).await.is_err());
}

// =============================================================================
// Test: DO fast-forward catchup for general festival data
// =============================================================================

#[tokio::test]
async fn test_do_fastforward_general_data() {
    let server = DevServer::start().await;
    use offbeat_core::signing;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let festival_id = "ff-general-test-1";
    let topic = format!("festival/{festival_id}/state");

    let admin_key = register_festival_admin(&server, festival_id).await;
    let mut public_key = None;

    let updates = vec![
        ("day1_headliner", "Radiohead"),
        ("day2_headliner", "Bjork"),
        ("day3_headliner", "Massive Attack"),
    ];

    for (key, value) in &updates {
        let doc = Doc::new();
        let map = doc.get_or_insert_map("root");
        {
            let mut txn = doc.transact_mut();
            map.insert(&mut txn, *key, *value);
        }
        let update_bytes = doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let response =
            publish_festival_update(&server, festival_id, &admin_key, &update_bytes).await;
        public_key = Some(
            hex_to_bytes(response["publicKey"].as_str().unwrap())
                .try_into()
                .unwrap(),
        );
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Late joiner N2 connects and catches up
    let (mut sink_n2, mut stream_n2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_sv_exchange(
        &mut sink_n2,
        &topic,
        yrs::StateVector::default().encode_v1(),
    )
    .await
    .unwrap();

    let checkpoint = wait_for_gossip(&mut stream_n2, 5).await.unwrap();
    let checkpoint = match checkpoint.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed checkpoint, got {other:?}"),
    };
    assert_eq!(checkpoint.kind, 2);
    let signed = checkpoint.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &public_key.unwrap(),
        &checkpoint.doc_id,
        checkpoint.kind,
        checkpoint.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let doc_n2 = Doc::new();
    let map_n2 = doc_n2.get_or_insert_map("root");
    doc_n2
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(&signed.update).unwrap())
        .unwrap();

    let txn = doc_n2.transact();
    for (key, value) in &updates {
        match map_n2.get(&txn, key) {
            Some(yrs::Out::Any(yrs::any::Any::String(s))) => assert_eq!(s.to_string(), *value),
            other => panic!("expected '{value}' for key '{key}', got {other:?}"),
        }
    }
}

// =============================================================================
// Test: DO fast-forward catchup for encrypted group data
// =============================================================================

#[tokio::test]
async fn test_do_fastforward_group_data() {
    let server = DevServer::start().await;
    use offbeat_core::{OffbeatNode, crypto};

    let festival_id = "ff-group-test-1";

    // N1 creates a group and makes several changes
    let node_n1 = OffbeatNode::new_in_memory().unwrap();
    let create = node_n1
        .group_manager
        .create_group(festival_id, "Catchup Crew", "n1-user", "N1")
        .await
        .unwrap();

    let group_id = &create.group_id;
    let group_key = node_n1.db.load_group_key(group_id).unwrap().unwrap();
    let doc_id = format!("group/{group_id}/state");
    let topic = doc_id.clone();

    // N1 adds pins and checks in
    node_n1
        .group_manager
        .add_pin(group_id, "pin-ff-1", "Bar Tent", "51.5,-0.1", "n1-user")
        .await
        .unwrap();
    node_n1
        .group_manager
        .add_pin(group_id, "pin-ff-2", "Main Gate", "51.6,-0.2", "n1-user")
        .await
        .unwrap();
    node_n1
        .group_manager
        .check_in(group_id, "n1-user", Some("arena"), None)
        .await
        .unwrap();

    let (mut sink_n1, _stream_n1) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    // N1 sends full state as an encrypted group_update
    let full_state = node_n1.doc_manager.encode_full_state(&doc_id).unwrap();
    let encrypted = crypto::encrypt(&group_key, &full_state).unwrap();

    let envelope = proto::GossipEnvelope {
        payload: Some(proto::gossip_envelope::Payload::GroupUpdate(
            proto::GroupUpdate {
                doc_id: doc_id.clone(),
                encrypted: encrypted.clone(),
                group_key_id: crypto::group_id_from_key(&group_key),
            },
        )),
    };
    send_gossip(&mut sink_n1, &topic, envelope).await.unwrap();

    tokio::time::sleep(Duration::from_millis(200)).await;

    // Late joiner N2 connects and catches up via DO
    let (mut sink_n2, mut stream_n2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_catchup(&mut sink_n2, &topic, 0).await.unwrap();

    let catchup = wait_for_catchup(&mut stream_n2).await.unwrap();
    assert!(
        !catchup.messages.is_empty(),
        "catchup should have at least 1 group_update"
    );

    // N2 has the group key (simulating join via invite)
    let node_n2 = OffbeatNode::new_in_memory().unwrap();
    node_n2
        .group_manager
        .join_group(&create.invite_payload, "n2-user", "N2")
        .await
        .unwrap();

    // Apply all caught-up group_update messages
    for entry in &catchup.messages {
        let gu = match entry.message.as_ref().and_then(|m| m.payload.as_ref()) {
            Some(proto::gossip_envelope::Payload::GroupUpdate(gu)) => gu,
            _ => continue,
        };
        let decrypted = crypto::decrypt(&group_key, &gu.encrypted).unwrap();
        node_n2
            .doc_manager
            .apply_update(&doc_id, &decrypted)
            .unwrap();
    }

    // N2 should now see all of N1's data
    let state_n2 = node_n2
        .group_manager
        .get_group_state(group_id)
        .await
        .unwrap();
    assert_eq!(state_n2.pins.len(), 2, "N2 should see both pins");
    let labels: Vec<&str> = state_n2.pins.iter().map(|p| p.label.as_str()).collect();
    assert!(labels.contains(&"Bar Tent"));
    assert!(labels.contains(&"Main Gate"));

    let n1_member = state_n2.members.iter().find(|m| m.user_id == "n1-user");
    assert!(n1_member.is_some(), "N2 should see N1 as member");
    assert_eq!(n1_member.unwrap().stage_id.as_deref(), Some("arena"));
}

// =============================================================================
// Test: N1 <-> N2 <-WS-> DO: N1 has signing key, sends signed update,
//       DO relays, N2 receives, AND the DO stores it so the HTTP advisory
//       endpoint (catchup) returns the signed data for any new joiner.
// =============================================================================

#[tokio::test]
async fn test_signed_update_via_do_advisory_catchup() {
    let server = DevServer::start().await;
    use offbeat_core::signing;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let festival_id = "advisory-test-1";
    let topic = format!("festival/{festival_id}/state");
    let admin_key = register_festival_admin(&server, festival_id).await;
    let (_live_sink, mut live_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    let source = Doc::new();
    let map = source.get_or_insert_map("root");
    map.insert(
        &mut source.transact_mut(),
        "set/1",
        "DJ Shadow @ Main Stage 22:00",
    );
    let update = source
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let response = publish_festival_update(&server, festival_id, &admin_key, &update).await;
    let public_key: [u8; 32] = hex_to_bytes(response["publicKey"].as_str().unwrap())
        .try_into()
        .unwrap();

    let live = wait_for_gossip(&mut live_stream, 5).await.unwrap();
    let live = match live.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed delta, got {other:?}"),
    };
    let signed = live.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &public_key,
        &live.doc_id,
        live.kind,
        live.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let (mut late_sink, mut late_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_sv_exchange(&mut late_sink, &topic, StateVector::default().encode_v1())
        .await
        .unwrap();
    let checkpoint = wait_for_gossip(&mut late_stream, 5).await.unwrap();
    let checkpoint = match checkpoint.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed checkpoint, got {other:?}"),
    };
    assert_eq!(checkpoint.kind, 2);
    let signed = checkpoint.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &public_key,
        &checkpoint.doc_id,
        checkpoint.kind,
        checkpoint.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let restored = Doc::new();
    let map = restored.get_or_insert_map("root");
    restored
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(&signed.update).unwrap())
        .unwrap();
    let txn = restored.transact();
    match map.get(&txn, "set/1") {
        Some(yrs::Out::Any(yrs::any::Any::String(value))) => {
            assert_eq!(value.to_string(), "DJ Shadow @ Main Stage 22:00");
        }
        other => panic!("expected set/1, got {other:?}"),
    }
}

// =============================================================================
// Test: DO public-key HTTP endpoint returns a valid hex key
// =============================================================================

#[tokio::test]
async fn test_do_public_key_endpoint() {
    let server = DevServer::start().await;
    let festival_id = "pubkey-test-1";
    let base_url = server.http_url();

    // First, trigger DO instantiation by connecting via WS
    let (sink, stream) = connect_and_auth(&server, festival_id).await.unwrap();
    drop(sink);
    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Fetch the public key via HTTP
    let url = format!("{base_url}/festivals/{festival_id}/public-key");
    let resp = reqwest::get(&url).await.unwrap();
    assert_eq!(resp.status(), 200);

    let hex_key = resp.text().await.unwrap();
    assert_eq!(
        hex_key.len(),
        64,
        "Ed25519 public key should be 64 hex chars, got {}",
        hex_key.len()
    );

    // Parse it as valid hex → 32 bytes
    let key_bytes: Vec<u8> = (0..hex_key.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex_key[i..i + 2], 16).unwrap())
        .collect();
    assert_eq!(key_bytes.len(), 32);

    // Key should be stable (same DO instance)
    let resp2 = reqwest::get(&url).await.unwrap();
    let hex_key2 = resp2.text().await.unwrap();
    assert_eq!(
        hex_key, hex_key2,
        "public key should be stable across requests"
    );
}

// =============================================================================
// Test: Partial catchup — sinceSeq skips already-seen messages
// =============================================================================

#[tokio::test]
async fn test_partial_catchup_since_seq() {
    let server = DevServer::start().await;
    let festival_id = "partial-catchup-test-1";
    let topic = format!("festival/{festival_id}/chat/general");

    let (mut sink, _stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    // Send 5 messages
    for i in 0..5 {
        let msg_id = uuid::Uuid::new_v4().to_string();
        let envelope = chat_envelope(&msg_id, "user1", "User", &format!("msg {i}"), &topic);
        send_gossip(&mut sink, &topic, envelope).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // First catchup from 0 — should get all 5
    let (mut sink2, mut stream2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_catchup(&mut sink2, &topic, 0).await.unwrap();
    let catchup1 = wait_for_catchup(&mut stream2).await.unwrap();
    assert!(
        catchup1.messages.len() >= 5,
        "full catchup should have >= 5 messages"
    );

    // Get the seq of message 3 (0-indexed)
    let seq_3 = catchup1.messages[2].seq;

    // Catchup since seq_3 — should only get messages after that
    send_catchup(&mut sink2, &topic, seq_3).await.unwrap();
    let catchup2 = wait_for_catchup(&mut stream2).await.unwrap();

    // Should have fewer messages (only those after seq_3)
    assert!(
        catchup2.messages.len() < catchup1.messages.len(),
        "partial catchup should return fewer messages"
    );
    for msg in &catchup2.messages {
        assert!(msg.seq > seq_3, "all messages should be after sinceSeq");
    }
}

// =============================================================================
// Test: Three-node relay — N1 sends signed update, N2 and N3 both receive,
//       both can verify the original signature
// =============================================================================

#[tokio::test]
async fn test_three_node_signed_relay() {
    let server = DevServer::start().await;
    use offbeat_core::signing;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let festival_id = "three-node-test-1";
    let topic = format!("festival/{festival_id}/state");

    let admin_key = register_festival_admin(&server, festival_id).await;

    let doc = Doc::new();
    let map = doc.get_or_insert_map("root");
    {
        let mut txn = doc.transact_mut();
        map.insert(&mut txn, "announcement", "Gates open at 10am");
    }
    let update_bytes = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let (_sink_n2, mut stream_n2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    let (_sink_n3, mut stream_n3) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    let response = publish_festival_update(&server, festival_id, &admin_key, &update_bytes).await;
    let public_key: [u8; 32] = hex_to_bytes(response["publicKey"].as_str().unwrap())
        .try_into()
        .unwrap();

    let recv_n2 = wait_for_gossip(&mut stream_n2, 5).await.unwrap();
    let recv_n3 = wait_for_gossip(&mut stream_n3, 5).await.unwrap();

    for (label, recv) in [("N2", &recv_n2), ("N3", &recv_n3)] {
        let fu = match recv.message.as_ref().unwrap().payload.as_ref().unwrap() {
            proto::gossip_envelope::Payload::FestivalUpdate(fu) => fu,
            other => panic!("{label}: expected festival_update, got {other:?}"),
        };
        let su = fu.signed_update.as_ref().unwrap();
        assert!(signing::verify_festival_update(
            &public_key,
            &fu.doc_id,
            fu.kind,
            fu.authority_seq,
            &su.update,
            &su.signature,
        ));

        let d = Doc::new();
        let m = d.get_or_insert_map("root");
        {
            let mut txn = d.transact_mut();
            txn.apply_update(yrs::Update::decode_v1(&su.update).unwrap())
                .unwrap();
        }
        let txn = d.transact();
        match m.get(&txn, "announcement") {
            Some(yrs::Out::Any(yrs::any::Any::String(s))) => {
                assert_eq!(s.to_string(), "Gates open at 10am");
            }
            other => panic!("{label}: expected announcement, got {other:?}"),
        }
    }
}

// =============================================================================
// Test: Encrypted group chat catchup — messages stored in DO, late joiner
//       catches up and can decrypt all messages
// =============================================================================

#[tokio::test]
async fn test_encrypted_group_chat_catchup() {
    let server = DevServer::start().await;
    use offbeat_core::crypto;
    use offbeat_core::types::ChatMessage;

    let festival_id = "group-chat-catchup-1";
    let group_key = crypto::generate_group_key();
    let group_id = crypto::group_id_from_key(&group_key);
    let topic = format!("group/{group_id}/chat");

    let (mut sink_d1, _stream_d1) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    let mut expected_texts = Vec::new();
    for i in 1..=3 {
        let text = format!("secret message {i}");
        expected_texts.push(text.clone());

        let msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: "d1-user".to_string(),
            display_name: "D1".to_string(),
            text,
            topic: topic.clone(),
            stage_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            writer_seq: 0,
        };
        let plaintext = serde_json::to_vec(&msg).unwrap();
        let encrypted = crypto::encrypt(&group_key, &plaintext).unwrap();

        let envelope = proto::GossipEnvelope {
            payload: Some(proto::gossip_envelope::Payload::EncryptedChat(
                proto::EncryptedPayload {
                    encrypted,
                    group_key_id: group_id.clone(),
                },
            )),
        };
        send_gossip(&mut sink_d1, &topic, envelope).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    tokio::time::sleep(Duration::from_millis(200)).await;

    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_catchup(&mut sink_d2, &topic, 0).await.unwrap();

    let catchup = wait_for_catchup(&mut stream_d2).await.unwrap();

    let encrypted_chats: Vec<_> = catchup
        .messages
        .iter()
        .filter(|e| {
            matches!(
                e.message.as_ref().and_then(|m| m.payload.as_ref()),
                Some(proto::gossip_envelope::Payload::EncryptedChat(_))
            )
        })
        .collect();
    assert_eq!(
        encrypted_chats.len(),
        3,
        "should catch up all 3 encrypted chat messages"
    );

    let mut decrypted_texts = Vec::new();
    for entry in &encrypted_chats {
        let ec = match entry.message.as_ref().unwrap().payload.as_ref().unwrap() {
            proto::gossip_envelope::Payload::EncryptedChat(ec) => ec,
            _ => unreachable!(),
        };
        let pt = crypto::decrypt(&group_key, &ec.encrypted).unwrap();
        let chat: ChatMessage = serde_json::from_slice(&pt).unwrap();
        decrypted_texts.push(chat.text);
    }
    decrypted_texts.sort();
    expected_texts.sort();
    assert_eq!(decrypted_texts, expected_texts);

    // Without the key, decryption fails
    let wrong_key = crypto::generate_group_key();
    let first_ec = match encrypted_chats[0]
        .message
        .as_ref()
        .unwrap()
        .payload
        .as_ref()
        .unwrap()
    {
        proto::gossip_envelope::Payload::EncryptedChat(ec) => ec,
        _ => unreachable!(),
    };
    assert!(crypto::decrypt(&wrong_key, &first_ec.encrypted).is_err());
}

// =============================================================================
// Test: Mixed traffic — chat and CRDT updates on different topics coexist
// =============================================================================

#[tokio::test]
async fn test_mixed_chat_and_crdt_traffic() {
    let server = DevServer::start().await;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let festival_id = "mixed-traffic-test-1";
    let chat_topic = format!("festival/{festival_id}/chat/main-stage");
    let state_topic = format!("festival/{festival_id}/state");

    let (mut sink_d1, mut _stream_d1) = connect_and_auth(&server, festival_id).await.unwrap();
    send_subscribe(&mut sink_d1, &[&chat_topic, &state_topic])
        .await
        .unwrap();
    wait_for_subscribed(&mut _stream_d1).await.unwrap();

    let (mut _sink_d2, mut stream_d2) = connect_and_auth(&server, festival_id).await.unwrap();
    send_subscribe(&mut _sink_d2, &[&chat_topic, &state_topic])
        .await
        .unwrap();
    wait_for_subscribed(&mut stream_d2).await.unwrap();

    let admin_key = register_festival_admin(&server, festival_id).await;

    let doc = Doc::new();
    let map = doc.get_or_insert_map("root");
    {
        let mut txn = doc.transact_mut();
        map.insert(&mut txn, "schedule_version", "v2");
    }
    let update_bytes = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    publish_festival_update(&server, festival_id, &admin_key, &update_bytes).await;

    let chat_msg_id = uuid::Uuid::new_v4().to_string();
    let chat_env = chat_envelope(
        &chat_msg_id,
        "d1",
        "D1",
        "Check the new schedule!",
        &chat_topic,
    );
    send_gossip(&mut sink_d1, &chat_topic, chat_env)
        .await
        .unwrap();

    // D2 receives both (order may vary)
    let mut has_festival = false;
    let mut has_chat = false;
    for _ in 0..2 {
        let recv = wait_for_gossip(&mut stream_d2, 5).await.unwrap();
        match recv.message.as_ref().unwrap().payload.as_ref().unwrap() {
            proto::gossip_envelope::Payload::FestivalUpdate(_) => has_festival = true,
            proto::gossip_envelope::Payload::Chat(_) => has_chat = true,
            _ => {}
        }
    }
    assert!(has_festival, "should receive festival_update");
    assert!(has_chat, "should receive chat");
}

// =============================================================================
// Test: Register admin, export DO signing key, sign update locally, gossip it,
//       late joiner catches up and verifies against DO's public key.
//       This exercises the REAL DO keypair end-to-end.
// =============================================================================

#[tokio::test]
async fn test_do_real_keypair_sign_and_catchup() {
    let server = DevServer::start().await;
    use offbeat_core::signing;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let festival_id = "do-real-key-test-1";
    let base_url = server.http_url();
    let client = reqwest::Client::new();

    let admin_key = signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());

    let (sink_init, stream_init) = connect_and_auth(&server, festival_id).await.unwrap();
    drop(sink_init);
    drop(stream_init);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let resp = client
        .put(format!("{base_url}/festivals/{festival_id}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "admin registration should succeed");

    let sig = signing::sign(&admin_key, b"export-signing-key");
    let sig_hex = bytes_to_hex(&sig);
    let resp = client
        .post(format!("{base_url}/festivals/{festival_id}/signing-key"))
        .json(&json!({ "publicKey": admin_pub_hex, "signature": sig_hex }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "signing key export should succeed");
    let do_secret_hex = resp.text().await.unwrap();
    assert_eq!(do_secret_hex.len(), 64);

    let do_secret: [u8; 32] = hex_to_bytes(&do_secret_hex).try_into().unwrap();
    let do_signing_key = ed25519_dalek::SigningKey::from_bytes(&do_secret);

    let resp = client
        .get(format!("{base_url}/festivals/{festival_id}/public-key"))
        .send()
        .await
        .unwrap();
    let do_pub_hex = resp.text().await.unwrap();
    let do_public_key: [u8; 32] = hex_to_bytes(&do_pub_hex).try_into().unwrap();
    assert_eq!(do_signing_key.verifying_key().to_bytes(), do_public_key);

    use base64::Engine as _;
    let topic = format!("festival/{festival_id}/state");
    let (_live_sink, mut live_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    let doc = Doc::new();
    let map = doc.get_or_insert_map("root");
    {
        let mut txn = doc.transact_mut();
        map.insert(&mut txn, "headliner_day1", "Aphex Twin");
        map.insert(&mut txn, "headliner_day2", "Autechre");
    }
    let update_bytes = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let auth_sig = signing::sign(&admin_key, format!("sign-update:{topic}").as_bytes());
    client
        .post(format!("{base_url}/festivals/{festival_id}/sign-update"))
        .json(&json!({
            "publicKey": admin_pub_hex,
            "signature": bytes_to_hex(&auth_sig),
            "docId": topic,
            "topic": topic,
            "update": base64::engine::general_purpose::STANDARD.encode(update_bytes),
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let live = wait_for_gossip(&mut live_stream, 5).await.unwrap();
    let live = match live.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed delta, got {other:?}"),
    };
    let signed = live.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &do_public_key,
        &live.doc_id,
        live.kind,
        live.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let (mut late_sink, mut late_stream) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_sv_exchange(
        &mut late_sink,
        &topic,
        yrs::StateVector::default().encode_v1(),
    )
    .await
    .unwrap();
    let checkpoint = wait_for_gossip(&mut late_stream, 5).await.unwrap();
    let checkpoint = match checkpoint.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected signed checkpoint, got {other:?}"),
    };
    assert_eq!(checkpoint.kind, 2);
    let signed = checkpoint.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &do_public_key,
        &checkpoint.doc_id,
        checkpoint.kind,
        checkpoint.authority_seq,
        &signed.update,
        &signed.signature,
    ));

    let restored = Doc::new();
    let restored_map = restored.get_or_insert_map("root");
    restored
        .transact_mut()
        .apply_update(yrs::Update::decode_v1(&signed.update).unwrap())
        .unwrap();
    let txn = restored.transact();
    match restored_map.get(&txn, "headliner_day1") {
        Some(yrs::Out::Any(yrs::any::Any::String(value))) => {
            assert_eq!(value.to_string(), "Aphex Twin")
        }
        other => panic!("expected checkpoint state, got {other:?}"),
    }
}

// =============================================================================
// Test: POST /sign-update — DO signs the update itself, broadcasts it,
//       WS subscriber receives it, late joiner catches up.
// =============================================================================

#[tokio::test]
async fn test_do_sign_update_endpoint() {
    let server = DevServer::start().await;
    use base64::Engine as _;
    use offbeat_core::signing;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let festival_id = "do-sign-update-test-1";
    let base_url = server.http_url();
    let client = reqwest::Client::new();
    let topic = format!("festival/{festival_id}/state");
    let doc_id = topic.clone();

    let admin_key = signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());

    let (s, r) = connect_and_auth(&server, festival_id).await.unwrap();
    drop(s);
    drop(r);
    tokio::time::sleep(Duration::from_millis(200)).await;

    client
        .put(format!("{base_url}/festivals/{festival_id}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();

    let (_sink_ws, mut stream_ws) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();

    let doc = Doc::new();
    let map = doc.get_or_insert_map("root");
    {
        let mut txn = doc.transact_mut();
        map.insert(&mut txn, "weather", "sunny");
        map.insert(&mut txn, "wind", "calm");
    }
    let update_bytes = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    let engine = base64::engine::general_purpose::STANDARD;

    let auth_msg = format!("sign-update:{doc_id}");
    let auth_sig = signing::sign(&admin_key, auth_msg.as_bytes());
    let auth_sig_hex = bytes_to_hex(&auth_sig);

    let resp = client
        .post(format!("{base_url}/festivals/{festival_id}/sign-update"))
        .json(&json!({
            "publicKey": admin_pub_hex,
            "signature": auth_sig_hex,
            "docId": doc_id,
            "topic": topic,
            "update": engine.encode(&update_bytes),
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "sign-update should succeed");

    let resp_body: Value = resp.json().await.unwrap();
    let response_kind = resp_body["kind"].as_i64().unwrap() as i32;
    let response_seq = resp_body["authoritySeq"]
        .as_str()
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let do_pub_hex = resp_body["publicKey"].as_str().unwrap();
    let do_public_key: [u8; 32] = hex_to_bytes(do_pub_hex).try_into().unwrap();

    let signed = &resp_body["signedUpdate"];
    let resp_update_bytes = engine.decode(signed["update"].as_str().unwrap()).unwrap();
    let resp_sig_bytes = engine
        .decode(signed["signature"].as_str().unwrap())
        .unwrap();
    assert!(signing::verify_festival_update(
        &do_public_key,
        &doc_id,
        response_kind,
        response_seq,
        &resp_update_bytes,
        &resp_sig_bytes,
    ));

    // The WS subscriber should have received it as binary protobuf
    let recv = wait_for_gossip(&mut stream_ws, 5).await.unwrap();
    let fu = match recv.message.as_ref().unwrap().payload.as_ref().unwrap() {
        proto::gossip_envelope::Payload::FestivalUpdate(fu) => fu,
        other => panic!("expected festival_update, got {other:?}"),
    };
    let su = fu.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &do_public_key,
        &fu.doc_id,
        fu.kind,
        fu.authority_seq,
        &su.update,
        &su.signature,
    ));

    let doc_ws = Doc::new();
    let map_ws = doc_ws.get_or_insert_map("root");
    {
        let mut txn = doc_ws.transact_mut();
        txn.apply_update(yrs::Update::decode_v1(&su.update).unwrap())
            .unwrap();
    }
    let txn = doc_ws.transact();
    match map_ws.get(&txn, "weather") {
        Some(yrs::Out::Any(yrs::any::Any::String(s))) => assert_eq!(s.to_string(), "sunny"),
        other => panic!("expected 'sunny', got {other:?}"),
    }

    drop(_sink_ws);
    drop(stream_ws);
    tokio::time::sleep(Duration::from_millis(200)).await;

    let (mut sink_late, mut stream_late) = connect_and_subscribe(&server, festival_id, &topic)
        .await
        .unwrap();
    send_sv_exchange(
        &mut sink_late,
        &topic,
        yrs::StateVector::default().encode_v1(),
    )
    .await
    .unwrap();

    let catchup = wait_for_gossip(&mut stream_late, 5).await.unwrap();
    let checkpoint = match catchup.message.and_then(|message| message.payload) {
        Some(proto::gossip_envelope::Payload::FestivalUpdate(update)) => update,
        other => panic!("expected checkpoint, got {other:?}"),
    };
    assert_eq!(checkpoint.kind, 2);
    let signed = checkpoint.signed_update.as_ref().unwrap();
    assert!(signing::verify_festival_update(
        &do_public_key,
        &checkpoint.doc_id,
        checkpoint.kind,
        checkpoint.authority_seq,
        &signed.update,
        &signed.signature,
    ));
}

// =============================================================================
// Test: Non-admin cannot export signing key or sign updates
// =============================================================================

#[tokio::test]
async fn test_do_signing_key_rejected_for_non_admin() {
    let server = DevServer::start().await;
    use offbeat_core::signing;

    let festival_id = "do-auth-reject-test-1";
    let base_url = server.http_url();
    let client = reqwest::Client::new();

    // Trigger DO init
    let (s, r) = connect_and_auth(&server, festival_id).await.unwrap();
    drop(s);
    drop(r);
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Register a real admin first
    let admin_key = signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());
    client
        .put(format!("{base_url}/festivals/{festival_id}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();

    // Non-admin tries to export signing key
    let intruder_key = signing::generate_signing_key();
    let intruder_pub_hex = bytes_to_hex(&intruder_key.verifying_key().to_bytes());
    let sig = signing::sign(&intruder_key, b"export-signing-key");
    let sig_hex = bytes_to_hex(&sig);

    let resp = client
        .post(format!("{base_url}/festivals/{festival_id}/signing-key"))
        .json(&json!({
            "publicKey": intruder_pub_hex,
            "signature": sig_hex,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "non-admin should be rejected");

    // Non-admin tries to sign an update
    let resp = client
        .post(format!("{base_url}/festivals/{festival_id}/sign-update"))
        .json(&json!({
            "publicKey": intruder_pub_hex,
            "signature": sig_hex,
            "docId": "festival/test",
            "topic": "festival/test/state",
            "update": "AAAA",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        403,
        "non-admin sign-update should be rejected"
    );

    // Admin with wrong signature should also fail (401, not 403)
    let wrong_sig = signing::sign(&admin_key, b"wrong-challenge");
    let wrong_sig_hex = bytes_to_hex(&wrong_sig);
    let resp = client
        .post(format!("{base_url}/festivals/{festival_id}/signing-key"))
        .json(&json!({
            "publicKey": admin_pub_hex,
            "signature": wrong_sig_hex,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "admin with wrong sig should get 401");
}

// =============================================================================
// Test: Global admins on MainDO are inherited by Festival DO
// =============================================================================

#[tokio::test]
async fn test_global_admins_inherited_by_festival_do() {
    let server = DevServer::start().await;
    use offbeat_core::signing;

    let festival_id = "global-admin-test-1";
    let base_url = server.http_url();
    let client = reqwest::Client::new();

    // Register a global admin on MainDO
    let admin_key = signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());

    let resp = client
        .put(format!("{base_url}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "global admin registration should succeed"
    );

    // Verify it's listed
    let resp = client
        .get(format!("{base_url}/admins"))
        .send()
        .await
        .unwrap();
    let admins: Vec<String> = resp.json().await.unwrap();
    assert!(admins.contains(&admin_pub_hex));

    // Connect to a Festival DO via WS — this triggers ensureFestivalConfig
    // which syncs global admins to the Festival DO
    let (sink_init, stream_init) = connect_and_auth(&server, festival_id).await.unwrap();
    drop(sink_init);
    drop(stream_init);
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Now the global admin should be able to export the Festival DO's signing key
    let sig = signing::sign(&admin_key, b"export-signing-key");
    let sig_hex = bytes_to_hex(&sig);

    let resp = client
        .post(format!("{base_url}/festivals/{festival_id}/signing-key"))
        .json(&json!({
            "publicKey": admin_pub_hex,
            "signature": sig_hex,
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        200,
        "global admin should be able to export festival signing key"
    );
    let key_hex = resp.text().await.unwrap();
    assert_eq!(key_hex.len(), 64);
}

// =============================================================================
// Helpers for admin-authenticated requests
// =============================================================================

/// Build auth headers: X-Admin-Key (hex pubkey) and X-Admin-Sig (hex sig over path).
fn admin_headers(key: &ed25519_dalek::SigningKey, path: &str) -> reqwest::header::HeaderMap {
    use offbeat_core::signing;
    use reqwest::header::{HeaderMap, HeaderValue};

    let pub_hex = bytes_to_hex(&key.verifying_key().to_bytes());
    let sig = signing::sign(key, path.as_bytes());
    let sig_hex = bytes_to_hex(&sig);

    let mut headers = HeaderMap::new();
    headers.insert("X-Admin-Key", HeaderValue::from_str(&pub_hex).unwrap());
    headers.insert("X-Admin-Sig", HeaderValue::from_str(&sig_hex).unwrap());
    headers
}

// =============================================================================
// Test: Create festival via POST, read it back, publish lineup
// =============================================================================

#[tokio::test]
async fn test_create_festival_and_publish_lineup() {
    let server = DevServer::start().await;
    let base = server.http_url();
    let client = reqwest::Client::new();

    // Register admin
    let admin_key = offbeat_core::signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());
    client
        .put(format!("{base}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();

    // Create festival
    let resp = client
        .post(format!("{base}/festivals"))
        .headers(admin_headers(&admin_key, "/festivals"))
        .json(&json!({
            "source": {
                "festivalId": "testfest-1",
                "clashfinderId": "fieldday2026",
                "name": "Test Festival",
                "location": "Hyde Park",
                "city": "London",
                "country": "GB",
                "genres": ["Rock", "Electronic"]
            }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "festival creation should succeed");

    let created: Value = resp.json().await.unwrap();
    assert_eq!(created["festival"]["id"], "testfest-1");
    assert_eq!(created["festival"]["name"], "Test Festival");
    assert_eq!(created["festival"]["city"], "London");
    let initial_set_count = created["lineup"]["sets"].as_array().unwrap().len();
    assert!(initial_set_count > 0);

    // Read it back via GET
    let resp = client
        .get(format!("{base}/festivals/testfest-1"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let fetched: Value = resp.json().await.unwrap();
    assert_eq!(fetched["name"], "Test Festival");

    // It should appear in the festivals list
    let resp = client
        .get(format!("{base}/festivals"))
        .send()
        .await
        .unwrap();
    let festivals: Vec<Value> = resp.json().await.unwrap();
    assert!(festivals.iter().any(|f| f["id"] == "testfest-1"));

    // Refresh from the configured Clashfinder source. This replaces rather
    // than appends the stored lineup.
    let resp = client
        .put(format!("{base}/festivals/testfest-1/lineup"))
        .headers(admin_headers(&admin_key, "/festivals/testfest-1/lineup"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "lineup refresh should succeed");
    let refreshed: Value = resp.json().await.unwrap();
    assert_eq!(
        refreshed["sets"].as_array().unwrap().len(),
        initial_set_count
    );

    let fetched: Value = client
        .get(format!("{base}/festivals/testfest-1/lineup"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fetched["sets"].as_array().unwrap().len(), initial_set_count);
}

// =============================================================================
// Test: Non-admin cannot create or update festivals
// =============================================================================

#[tokio::test]
async fn test_festival_crud_requires_admin() {
    let server = DevServer::start().await;
    let base = server.http_url();
    let client = reqwest::Client::new();

    // Register a real admin
    let admin_key = offbeat_core::signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());
    client
        .put(format!("{base}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();

    // Non-admin tries to create festival (no auth headers)
    let resp = client
        .post(format!("{base}/festivals"))
        .json(&json!({
            "id": "evil-fest",
            "name": "Evil Festival",
            "startDate": "2026-01-01",
            "endDate": "2026-01-02",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "no auth headers should get 401");

    // Non-admin with fake headers
    let intruder = offbeat_core::signing::generate_signing_key();
    let resp = client
        .post(format!("{base}/festivals"))
        .headers(admin_headers(&intruder, "/festivals"))
        .json(&json!({
            "id": "evil-fest",
            "name": "Evil Festival",
            "startDate": "2026-01-01",
            "endDate": "2026-01-02",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 403, "non-admin key should get 403");

    // Admin with wrong path signature
    let resp = client
        .post(format!("{base}/festivals"))
        .headers(admin_headers(&admin_key, "/wrong-path"))
        .json(&json!({
            "id": "evil-fest",
            "name": "Evil Festival",
            "startDate": "2026-01-01",
            "endDate": "2026-01-02",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401, "wrong path sig should get 401");
}

// =============================================================================
// Test: Update festival metadata and replace lineup
// =============================================================================

#[tokio::test]
async fn test_update_festival_and_replace_lineup() {
    let server = DevServer::start().await;
    let base = server.http_url();
    let client = reqwest::Client::new();

    // Register admin + create festival
    let admin_key = offbeat_core::signing::generate_signing_key();
    let admin_pub_hex = bytes_to_hex(&admin_key.verifying_key().to_bytes());
    client
        .put(format!("{base}/admins"))
        .json(&json!({ "publicKey": admin_pub_hex }))
        .send()
        .await
        .unwrap();

    let created: Value = client
        .post(format!("{base}/festivals"))
        .headers(admin_headers(&admin_key, "/festivals"))
        .json(&json!({
            "source": {
                "festivalId": "updatefest",
                "clashfinderId": "fieldday2026",
                "name": "Original Name",
                "location": "Victoria Park",
                "city": "London",
                "country": "GB"
            }
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let expected_sets = created["lineup"]["sets"].as_array().unwrap().len();
    assert!(expected_sets > 0);

    // Update metadata
    let resp = client
        .put(format!("{base}/festivals/updatefest"))
        .headers(admin_headers(&admin_key, "/festivals/updatefest"))
        .json(&json!({ "name": "Updated Name", "city": "Berlin" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let updated: Value = resp.json().await.unwrap();
    assert_eq!(updated["name"], "Updated Name");
    assert_eq!(updated["city"], "Berlin");

    // Refreshing the configured source replaces rows idempotently rather than
    // appending duplicate sets.
    for _ in 0..2 {
        let refreshed: Value = client
            .put(format!("{base}/festivals/updatefest/lineup"))
            .headers(admin_headers(&admin_key, "/festivals/updatefest/lineup"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(refreshed["sets"].as_array().unwrap().len(), expected_sets);
    }
}

// =============================================================================
// Test: Seeded festival (fieldday26) lineup is served from DB
// =============================================================================

#[tokio::test]
async fn test_fresh_registry_has_no_implicit_seed_data() {
    let server = DevServer::start().await;
    let base = server.http_url();
    let client = reqwest::Client::new();

    let festivals: Vec<Value> = client
        .get(format!("{base}/festivals"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(festivals.is_empty());

    let response = client
        .get(format!("{base}/festivals/fieldday26/lineup"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}
