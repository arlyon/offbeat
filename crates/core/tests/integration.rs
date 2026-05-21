//! Integration tests for offbeat-core against a running DO server.
//!
//! These tests require the DO server to be running. Start it with:
//!   cd apps/server && pnpm wrangler dev
//!
//! Or set the `OFFBEAT_SERVER_URL` environment variable to point to the server.
//!
//! Run the tests with:
//!   cargo test --test integration -- --ignored
//!
//! Or to run against a specific server:
//!   OFFBEAT_SERVER_URL=ws://127.0.0.1:8787 cargo test --test integration -- --ignored

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::time::timeout;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use yrs::updates::decoder::Decode;

/// Get the server URL from environment or use default localhost
fn server_url() -> String {
    std::env::var("OFFBEAT_SERVER_URL").unwrap_or_else(|_| "ws://127.0.0.1:8787".to_string())
}

/// Connect to a festival's WebSocket endpoint
async fn connect_to_festival(
    festival_id: &str,
) -> Result<
    (
        futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            Message,
        >,
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ),
    anyhow::Error,
> {
    let url = format!("{}/festivals/{}/ws", server_url(), festival_id);
    let (ws_stream, _) = connect_async(&url).await?;
    Ok(ws_stream.split())
}

/// Send a JSON message over WebSocket
async fn send_json<S>(sink: &mut S, value: &Value) -> Result<(), anyhow::Error>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let msg = Message::Text(value.to_string().into());
    sink.send(msg).await.map_err(|e| anyhow::anyhow!("{}", e))?;
    Ok(())
}

/// Receive a JSON message from WebSocket with timeout
#[allow(dead_code)]
async fn recv_json<S>(stream: &mut S, timeout_secs: u64) -> Result<Value, anyhow::Error>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let result = timeout(Duration::from_secs(timeout_secs), stream.next()).await?;
    match result {
        Some(Ok(Message::Text(text))) => Ok(serde_json::from_str(&text)?),
        Some(Ok(msg)) => anyhow::bail!("Unexpected message type: {:?}", msg),
        Some(Err(e)) => anyhow::bail!("WebSocket error: {}", e),
        None => anyhow::bail!("WebSocket closed unexpectedly"),
    }
}

/// Wait for a message with a specific type
async fn wait_for_message_type<S>(
    stream: &mut S,
    expected_type: &str,
    timeout_secs: u64,
) -> Result<Value, anyhow::Error>
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            anyhow::bail!("Timeout waiting for message type: {}", expected_type);
        }

        let msg = timeout(remaining, stream.next()).await?;
        match msg {
            Some(Ok(Message::Text(text))) => {
                let value: Value = serde_json::from_str(&text)?;
                if value.get("type").and_then(|t| t.as_str()) == Some(expected_type) {
                    return Ok(value);
                }
                // Not the type we're looking for, continue waiting
            }
            Some(Ok(_)) => continue,
            Some(Err(e)) => anyhow::bail!("WebSocket error: {}", e),
            None => anyhow::bail!("WebSocket closed unexpectedly"),
        }
    }
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

// =============================================================================
// Test: Single client connects and subscribes
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_single_client_subscribe() {
    let (mut sink, mut stream) = connect_to_festival("rust-test-1").await.unwrap();

    // Subscribe to a topic
    send_json(
        &mut sink,
        &json!({
            "type": "subscribe",
            "topics": ["festival/rust-test-1/chat"]
        }),
    )
    .await
    .unwrap();

    // Wait for subscribed response
    let response = wait_for_message_type(&mut stream, "subscribed", 5).await.unwrap();
    assert_eq!(response["type"], "subscribed");

    let topics = response["topics"].as_array().unwrap();
    assert!(topics.iter().any(|t| t == "festival/rust-test-1/chat"));
}

// =============================================================================
// Test: Single client sends update and retrieves via catchup
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_single_client_chat_and_catchup() {
    let (mut sink, mut stream) = connect_to_festival("rust-test-2").await.unwrap();

    // Subscribe
    send_json(
        &mut sink,
        &json!({
            "type": "subscribe",
            "topics": ["festival/rust-test-2/chat"]
        }),
    )
    .await
    .unwrap();
    wait_for_message_type(&mut stream, "subscribed", 5)
        .await
        .unwrap();

    // Send a chat message
    let msg_id = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink,
        &json!({
            "type": "chat",
            "topic": "festival/rust-test-2/chat",
            "message": {
                "id": msg_id,
                "userId": "rust-user-1",
                "displayName": "Rust User",
                "text": "Hello from Rust!",
                "topic": "festival/rust-test-2/chat",
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    // Small delay for message to be stored
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Request catchup
    send_json(
        &mut sink,
        &json!({
            "type": "catchup",
            "topic": "festival/rust-test-2/chat",
            "sinceSeq": 0
        }),
    )
    .await
    .unwrap();

    // Wait for catchup response
    let catchup = wait_for_message_type(&mut stream, "catchup", 5)
        .await
        .unwrap();
    assert_eq!(catchup["type"], "catchup");
    assert_eq!(catchup["topic"], "festival/rust-test-2/chat");

    let chat_msgs = catchup["chat"].as_array().unwrap();
    assert!(!chat_msgs.is_empty());

    // Find our message
    let our_msg = chat_msgs
        .iter()
        .find(|m| m["message"]["id"] == msg_id)
        .expect("Our message should be in catchup");
    assert_eq!(our_msg["message"]["text"], "Hello from Rust!");
}

// =============================================================================
// Test: Two clients - direct connect sends, p2p client receives via relay
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_two_clients_relay() {
    let topic = "festival/rust-test-3/chat";

    // Client A (direct) connects
    let (mut sink_a, mut stream_a) = connect_to_festival("rust-test-3").await.unwrap();

    // Client B (p2p simulation - receives via DO relay) connects
    let (mut sink_b, mut stream_b) = connect_to_festival("rust-test-3").await.unwrap();

    // Both subscribe to the same topic
    send_json(
        &mut sink_a,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();
    send_json(
        &mut sink_b,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();

    // Wait for both subscriptions
    wait_for_message_type(&mut stream_a, "subscribed", 5)
        .await
        .unwrap();
    wait_for_message_type(&mut stream_b, "subscribed", 5)
        .await
        .unwrap();

    // Client A sends a message
    let msg_id = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_a,
        &json!({
            "type": "chat",
            "topic": topic,
            "message": {
                "id": msg_id,
                "userId": "client-a",
                "displayName": "Client A",
                "text": "Message from A to B via DO relay",
                "topic": topic,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    // Client B should receive the message via DO relay
    let received = wait_for_message_type(&mut stream_b, "chat", 5)
        .await
        .unwrap();
    assert_eq!(received["type"], "chat");
    assert_eq!(received["topic"], topic);
    assert_eq!(received["message"]["text"], "Message from A to B via DO relay");
    assert_eq!(received["message"]["userId"], "client-a");
    assert!(received["seq"].as_i64().is_some());
}

// =============================================================================
// Test: P2P client sends relay update, direct client receives
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_p2p_relay_update() {
    let topic = "festival/rust-test-4/state";

    // Client A (direct)
    let (mut sink_a, mut stream_a) = connect_to_festival("rust-test-4").await.unwrap();

    // Client B (p2p simulation)
    let (mut sink_b, mut stream_b) = connect_to_festival("rust-test-4").await.unwrap();

    // Subscribe both
    send_json(
        &mut sink_a,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();
    send_json(
        &mut sink_b,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();

    wait_for_message_type(&mut stream_a, "subscribed", 5)
        .await
        .unwrap();
    wait_for_message_type(&mut stream_b, "subscribed", 5)
        .await
        .unwrap();

    // Client B sends a relay message (simulating CRDT update)
    let relay_data = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        b"crdt-update-from-p2p-client",
    );
    send_json(
        &mut sink_b,
        &json!({
            "type": "relay",
            "topic": topic,
            "data": relay_data
        }),
    )
    .await
    .unwrap();

    // Client A should receive it
    let received = wait_for_message_type(&mut stream_a, "relay", 5)
        .await
        .unwrap();
    assert_eq!(received["type"], "relay");
    assert_eq!(received["topic"], topic);
    assert_eq!(received["data"], relay_data);
    assert!(received["seq"].as_i64().is_some());
}

// =============================================================================
// Test: Yrs CRDT document sync via DO relay
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_yrs_crdt_sync_via_relay() {
    use base64::Engine;
    use yrs::{Doc, GetString, ReadTxn, Text, Transact};

    let topic = "festival/rust-test-5/state";

    // Client A creates a Yrs document and makes changes
    let doc_a = Doc::new();
    let text_a = doc_a.get_or_insert_text("content");
    {
        let mut txn = doc_a.transact_mut();
        text_a.insert(&mut txn, 0, "Hello from client A");
    }

    // Get the update to send
    let update_a = doc_a.transact().encode_state_as_update_v1(&yrs::StateVector::default());
    let encoded_update = base64::engine::general_purpose::STANDARD.encode(&update_a);

    // Connect both clients
    let (mut sink_a, mut stream_a) = connect_to_festival("rust-test-5").await.unwrap();
    let (mut sink_b, mut stream_b) = connect_to_festival("rust-test-5").await.unwrap();

    // Subscribe both
    send_json(
        &mut sink_a,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();
    send_json(
        &mut sink_b,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();

    wait_for_message_type(&mut stream_a, "subscribed", 5)
        .await
        .unwrap();
    wait_for_message_type(&mut stream_b, "subscribed", 5)
        .await
        .unwrap();

    // Client A sends the Yrs update via relay
    send_json(
        &mut sink_a,
        &json!({
            "type": "relay",
            "topic": topic,
            "data": encoded_update
        }),
    )
    .await
    .unwrap();

    // Client B receives the update
    let received = wait_for_message_type(&mut stream_b, "relay", 5)
        .await
        .unwrap();
    let received_data = received["data"].as_str().unwrap();

    // Decode and apply to client B's document
    let decoded_update = base64::engine::general_purpose::STANDARD
        .decode(received_data)
        .unwrap();

    let doc_b = Doc::new();
    let text_b = doc_b.get_or_insert_text("content");
    {
        let mut txn = doc_b.transact_mut();
        txn.apply_update(yrs::Update::decode_v1(&decoded_update).unwrap())
            .unwrap();
    }

    // Verify client B has the same content
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
#[ignore = "requires running DO server"]
async fn test_late_joiner_catchup() {
    let topic = "festival/rust-test-6/chat";

    // Client A connects and sends a message
    let (mut sink_a, mut stream_a) = connect_to_festival("rust-test-6").await.unwrap();

    send_json(
        &mut sink_a,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();
    wait_for_message_type(&mut stream_a, "subscribed", 5)
        .await
        .unwrap();

    // Send message before client B joins
    let msg_id = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_a,
        &json!({
            "type": "chat",
            "topic": topic,
            "message": {
                "id": msg_id,
                "userId": "early-user",
                "displayName": "Early User",
                "text": "Message sent before late joiner",
                "topic": topic,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    // Wait for message to be stored
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now client B (late joiner) connects
    let (mut sink_b, mut stream_b) = connect_to_festival("rust-test-6").await.unwrap();

    send_json(
        &mut sink_b,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await
    .unwrap();
    wait_for_message_type(&mut stream_b, "subscribed", 5)
        .await
        .unwrap();

    // Client B requests catchup
    send_json(
        &mut sink_b,
        &json!({
            "type": "catchup",
            "topic": topic,
            "sinceSeq": 0
        }),
    )
    .await
    .unwrap();

    // Client B should receive the missed message via catchup
    let catchup = wait_for_message_type(&mut stream_b, "catchup", 5)
        .await
        .unwrap();
    assert_eq!(catchup["type"], "catchup");

    let chat_msgs = catchup["chat"].as_array().unwrap();
    let our_msg = chat_msgs
        .iter()
        .find(|m| m["message"]["id"] == msg_id)
        .expect("Missed message should be in catchup");
    assert_eq!(our_msg["message"]["text"], "Message sent before late joiner");
}

// =============================================================================
// D1 ↔ D2 ↔ S1 relay scenario tests
//
// D1 = direct client (connects to the DO via WS)
// D2 = direct client (connects to the DO via WS)
// S1 = the DO server acting as relay
// =============================================================================

/// Connect to a festival WebSocket, subscribe to a topic, and return the
/// open sink/stream pair.
async fn connect_and_subscribe(
    festival_id: &str,
    topic: &str,
) -> Result<
    (
        futures_util::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
            tokio_tungstenite::tungstenite::Message,
        >,
        futures_util::stream::SplitStream<
            tokio_tungstenite::WebSocketStream<
                tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
            >,
        >,
    ),
    anyhow::Error,
> {
    let (mut sink, mut stream) = connect_to_festival(festival_id).await?;
    send_json(
        &mut sink,
        &json!({
            "type": "subscribe",
            "topics": [topic]
        }),
    )
    .await?;
    wait_for_message_type(&mut stream, "subscribed", 5).await?;
    Ok((sink, stream))
}

// =============================================================================
// Test: D1 sends chat → S1 relays → D2 receives; D2 sends → D1 receives
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_d1_d2_s1_relay_chat() {
    let topic = "festival/relay-test-1/chat";

    // D1 connects and subscribes
    let (mut sink_d1, mut stream_d1) = connect_and_subscribe("relay-test-1", topic)
        .await
        .unwrap();

    // D2 connects and subscribes
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe("relay-test-1", topic)
        .await
        .unwrap();

    // D1 → S1 → D2
    let msg_id_1 = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_d1,
        &json!({
            "type": "chat",
            "topic": topic,
            "message": {
                "id": msg_id_1,
                "userId": "d1",
                "displayName": "D1",
                "text": "Hello from D1",
                "topic": topic,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    let recv_d2 = wait_for_message_type(&mut stream_d2, "chat", 5)
        .await
        .unwrap();
    assert_eq!(recv_d2["message"]["text"], "Hello from D1");
    assert_eq!(recv_d2["message"]["userId"], "d1");

    // D2 → S1 → D1
    let msg_id_2 = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_d2,
        &json!({
            "type": "chat",
            "topic": topic,
            "message": {
                "id": msg_id_2,
                "userId": "d2",
                "displayName": "D2",
                "text": "Hello from D2",
                "topic": topic,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    let recv_d1 = wait_for_message_type(&mut stream_d1, "chat", 5)
        .await
        .unwrap();
    assert_eq!(recv_d1["message"]["text"], "Hello from D2");
    assert_eq!(recv_d1["message"]["userId"], "d2");
}

// =============================================================================
// Test: D1 sends encrypted CRDT relay → D2 receives and applies
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_d1_d2_s1_relay_crdt_update() {
    use base64::Engine as _;
    use offbeat_core::crypto;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    let topic = "festival/relay-test-2/state";

    // D1 and D2 share a group key (pre-shared out-of-band)
    let group_key = crypto::generate_group_key();

    // D1 creates a Yrs doc, makes a change, encrypts the update
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
    let encoded = base64::engine::general_purpose::STANDARD.encode(&encrypted);

    // D1 connects and subscribes
    let (mut sink_d1, _stream_d1) = connect_and_subscribe("relay-test-2", topic)
        .await
        .unwrap();

    // D2 connects and subscribes
    let (_sink_d2, mut stream_d2) = connect_and_subscribe("relay-test-2", topic)
        .await
        .unwrap();

    // D1 sends the encrypted CRDT update as a relay message
    send_json(
        &mut sink_d1,
        &json!({
            "type": "relay",
            "topic": topic,
            "data": encoded
        }),
    )
    .await
    .unwrap();

    // D2 receives the relay message
    let recv = wait_for_message_type(&mut stream_d2, "relay", 5)
        .await
        .unwrap();
    let received_data = recv["data"].as_str().unwrap();

    // D2 decodes and decrypts the CRDT update
    let received_encrypted = base64::engine::general_purpose::STANDARD
        .decode(received_data)
        .unwrap();
    let decrypted = crypto::decrypt(&group_key, &received_encrypted).unwrap();

    // D2 applies the update to its own doc
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
#[ignore = "requires running DO server"]
async fn test_d1_disconnect_d2_sends_d1_catchup() {
    let topic = "festival/relay-test-3/chat";

    // D1 connects, subscribes, and sends one message to establish a baseline
    let (mut sink_d1, stream_d1) = connect_and_subscribe("relay-test-3", topic)
        .await
        .unwrap();

    let initial_msg_id = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_d1,
        &json!({
            "type": "chat",
            "topic": topic,
            "message": {
                "id": initial_msg_id,
                "userId": "d1",
                "displayName": "D1",
                "text": "D1 initial message",
                "topic": topic,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    // Small delay for message to be stored
    tokio::time::sleep(Duration::from_millis(100)).await;

    // D1 disconnects (drop sink and stream)
    drop(sink_d1);
    drop(stream_d1);

    // D2 connects and sends 3 messages while D1 is offline
    let (mut sink_d2, _stream_d2) = connect_and_subscribe("relay-test-3", topic)
        .await
        .unwrap();

    let mut d2_msg_ids = Vec::new();
    for i in 1..=3 {
        let msg_id = uuid::Uuid::new_v4().to_string();
        d2_msg_ids.push(msg_id.clone());
        send_json(
            &mut sink_d2,
            &json!({
                "type": "chat",
                "topic": topic,
                "message": {
                    "id": msg_id,
                    "userId": "d2",
                    "displayName": "D2",
                    "text": format!("D2 message {i}"),
                    "topic": topic,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            }),
        )
        .await
        .unwrap();
    }

    // Wait for all D2 messages to be stored
    tokio::time::sleep(Duration::from_millis(200)).await;

    // D1 reconnects
    let (mut sink_d1_new, mut stream_d1_new) = connect_and_subscribe("relay-test-3", topic)
        .await
        .unwrap();

    // D1 requests catchup from seq 0 (full history)
    send_json(
        &mut sink_d1_new,
        &json!({
            "type": "catchup",
            "topic": topic,
            "sinceSeq": 0
        }),
    )
    .await
    .unwrap();

    // D1 receives the catchup
    let catchup = wait_for_message_type(&mut stream_d1_new, "catchup", 5)
        .await
        .unwrap();
    assert_eq!(catchup["type"], "catchup");

    let chat_msgs = catchup["chat"].as_array().unwrap();

    // Verify all 3 of D2's messages are present in the catchup
    for msg_id in &d2_msg_ids {
        let found = chat_msgs
            .iter()
            .any(|m| m["message"]["id"].as_str() == Some(msg_id));
        assert!(found, "D2 message {msg_id} not found in D1 catchup");
    }

    // Verify D1's initial message is also present
    let initial_found = chat_msgs
        .iter()
        .any(|m| m["message"]["id"].as_str() == Some(initial_msg_id.as_str()));
    assert!(
        initial_found,
        "D1 initial message not found in catchup"
    );

    // Cleanup
    drop(sink_d1_new);
    drop(sink_d2);
}

// =============================================================================
// Test: Group encrypted state sync via DO relay (with SV handshake)
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_group_encrypted_state_sync_via_relay() {
    use base64::Engine as _;
    use offbeat_core::{OffbeatNode, crypto};

    let festival_id = "relay-group-test-1";
    let topic = format!("festival/{festival_id}/state");

    // D1 creates a group.
    let node_d1 = OffbeatNode::new_in_memory().unwrap();
    let create_result = node_d1
        .group_manager
        .create_group(festival_id, "Test Crew", "d1-user", "D1 User")
        .await
        .unwrap();

    let group_id = &create_result.group_id;
    let group_key = node_d1.db.load_group_key(group_id).unwrap().unwrap();
    let doc_id = format!("group/{group_id}");

    // D1 adds a pin.
    node_d1
        .group_manager
        .add_pin(group_id, "pin-relay-1", "Tent Area", "51.5,-0.1", "d1-user")
        .await
        .unwrap();

    // D2 joins the group.
    let node_d2 = OffbeatNode::new_in_memory().unwrap();
    node_d2
        .group_manager
        .join_group(&create_result.invite_payload, "d2-user", "D2 User")
        .await
        .unwrap();

    // Connect both to the DO relay.
    let (mut sink_d1, mut stream_d1) = connect_and_subscribe(festival_id, &topic).await.unwrap();
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(festival_id, &topic).await.unwrap();

    // --- SV handshake: D2 → D1 → D2 ---

    // D2 sends its SV (encrypted) so D1 can compute a targeted diff.
    let encrypted_sv_d2 = node_d2.group_manager.request_group_sync(group_id).await.unwrap();
    let encoded_sv = base64::engine::general_purpose::STANDARD.encode(&encrypted_sv_d2);
    send_json(&mut sink_d2, &json!({ "type": "relay", "topic": topic, "data": encoded_sv })).await.unwrap();

    // D1 receives D2's SV, computes diff, sends back.
    let recv = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let sv_encrypted = base64::engine::general_purpose::STANDARD.decode(recv["data"].as_str().unwrap()).unwrap();
    let diff_for_d2 = node_d1.group_manager.handle_sync_request(group_id, &sv_encrypted).await.unwrap();
    let encoded_diff = base64::engine::general_purpose::STANDARD.encode(&diff_for_d2);
    send_json(&mut sink_d1, &json!({ "type": "relay", "topic": topic, "data": encoded_diff })).await.unwrap();

    // D2 receives diff and applies it.
    let recv_d2 = wait_for_message_type(&mut stream_d2, "relay", 5).await.unwrap();
    let diff_encrypted = base64::engine::general_purpose::STANDARD.decode(recv_d2["data"].as_str().unwrap()).unwrap();
    let diff_bytes = crypto::decrypt(&group_key, &diff_encrypted).unwrap();
    node_d2.doc_manager.lock().await.apply_update(&doc_id, &diff_bytes).unwrap();

    // D2 now has D1's pin.
    let state_d2 = node_d2.group_manager.get_group_state(group_id).await.unwrap();
    assert_eq!(state_d2.pins.len(), 1);
    assert_eq!(state_d2.pins[0].label, "Tent Area");

    // --- Reverse handshake: D1 → D2 → D1 (so D1 gets D2's member entry) ---

    let encrypted_sv_d1 = node_d1.group_manager.request_group_sync(group_id).await.unwrap();
    let encoded_sv_d1 = base64::engine::general_purpose::STANDARD.encode(&encrypted_sv_d1);
    send_json(&mut sink_d1, &json!({ "type": "relay", "topic": topic, "data": encoded_sv_d1 })).await.unwrap();

    let recv_d2_sv = wait_for_message_type(&mut stream_d2, "relay", 5).await.unwrap();
    let sv_d1_encrypted = base64::engine::general_purpose::STANDARD.decode(recv_d2_sv["data"].as_str().unwrap()).unwrap();
    let diff_for_d1 = node_d2.group_manager.handle_sync_request(group_id, &sv_d1_encrypted).await.unwrap();
    let encoded_diff_d1 = base64::engine::general_purpose::STANDARD.encode(&diff_for_d1);
    send_json(&mut sink_d2, &json!({ "type": "relay", "topic": topic, "data": encoded_diff_d1 })).await.unwrap();

    let recv_d1_diff = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let diff_d1_encrypted = base64::engine::general_purpose::STANDARD.decode(recv_d1_diff["data"].as_str().unwrap()).unwrap();
    let diff_d1_bytes = crypto::decrypt(&group_key, &diff_d1_encrypted).unwrap();
    node_d1.doc_manager.lock().await.apply_update(&doc_id, &diff_d1_bytes).unwrap();

    // --- Now both are synced. D2 checks in, sends diff. ---

    let encrypted_d2_checkin = node_d2
        .group_manager
        .check_in(group_id, "d2-user", Some("main-stage"), None)
        .await
        .unwrap();
    let encoded_checkin = base64::engine::general_purpose::STANDARD.encode(&encrypted_d2_checkin);
    send_json(&mut sink_d2, &json!({ "type": "relay", "topic": topic, "data": encoded_checkin })).await.unwrap();

    // D1 receives and applies the diff (works because they are synced).
    let recv_d1_checkin = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let checkin_encrypted = base64::engine::general_purpose::STANDARD.decode(recv_d1_checkin["data"].as_str().unwrap()).unwrap();
    let checkin_bytes = crypto::decrypt(&group_key, &checkin_encrypted).unwrap();
    node_d1.doc_manager.lock().await.apply_update(&doc_id, &checkin_bytes).unwrap();

    // D1 sees D2's location.
    let state_d1 = node_d1.group_manager.get_group_state(group_id).await.unwrap();
    let d2_member = state_d1.members.iter().find(|m| m.user_id == "d2-user").expect("d2-user should be in state");
    assert_eq!(d2_member.stage_id.as_deref(), Some("main-stage"));

    drop(sink_d1);
    drop(sink_d2);
}

// =============================================================================
// Test: Full SV handshake via DO relay
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_sv_handshake_group_sync() {
    use base64::Engine as _;
    use offbeat_core::{OffbeatNode, crypto};
    use offbeat_core::gossip_manager::GossipWireMessage;

    let festival_id = "sv-handshake-test-1";
    let topic = format!("festival/{festival_id}/state");

    // D1 creates a group and makes several changes.
    let node_d1 = OffbeatNode::new_in_memory().unwrap();
    let create_result = node_d1
        .group_manager
        .create_group(festival_id, "SV Crew", "d1-user", "D1 User")
        .await
        .unwrap();

    let group_id = &create_result.group_id;
    let group_key = node_d1.db.load_group_key(group_id).unwrap().unwrap();

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

    // D2 joins the group (has only its own member entry).
    let node_d2 = OffbeatNode::new_in_memory().unwrap();
    node_d2
        .group_manager
        .join_group(&create_result.invite_payload, "d2-user", "D2 User")
        .await
        .unwrap();

    // Connect both to the DO relay.
    let (mut sink_d1, mut stream_d1) = connect_and_subscribe(festival_id, &topic).await.unwrap();
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(festival_id, &topic).await.unwrap();

    // --- Step 1: D2 sends a sync_request (encrypted SV) ---
    let encrypted_sv = node_d2
        .group_manager
        .request_group_sync(group_id)
        .await
        .unwrap();

    // Wrap as a GossipWireMessage with kind="sync_request".
    let wire_req = GossipWireMessage {
        kind: "sync_request".to_string(),
        doc_id: Some(format!("group/{group_id}")),
        payload: base64::engine::general_purpose::STANDARD.encode(&encrypted_sv),
        group_key_id: Some(crypto::group_id_from_key(&group_key)),
    };
    let wire_req_bytes = serde_json::to_vec(&wire_req).unwrap();
    let encoded_req = base64::engine::general_purpose::STANDARD.encode(&wire_req_bytes);

    send_json(
        &mut sink_d2,
        &serde_json::json!({
            "type": "relay",
            "topic": topic,
            "data": encoded_req
        }),
    )
    .await
    .unwrap();

    // --- Step 2: D1 receives the sync_request, computes diff, sends sync_response ---
    let recv = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let received_bytes = base64::engine::general_purpose::STANDARD
        .decode(recv["data"].as_str().unwrap())
        .unwrap();
    let received_wire: GossipWireMessage = serde_json::from_slice(&received_bytes).unwrap();
    assert_eq!(received_wire.kind, "sync_request");

    let received_sv_encrypted = base64::engine::general_purpose::STANDARD
        .decode(&received_wire.payload)
        .unwrap();

    let encrypted_diff = node_d1
        .group_manager
        .handle_sync_request(group_id, &received_sv_encrypted)
        .await
        .unwrap();

    let wire_resp = GossipWireMessage {
        kind: "sync_response".to_string(),
        doc_id: Some(format!("group/{group_id}")),
        payload: base64::engine::general_purpose::STANDARD.encode(&encrypted_diff),
        group_key_id: Some(crypto::group_id_from_key(&group_key)),
    };
    let wire_resp_bytes = serde_json::to_vec(&wire_resp).unwrap();
    let encoded_resp = base64::engine::general_purpose::STANDARD.encode(&wire_resp_bytes);

    send_json(
        &mut sink_d1,
        &serde_json::json!({
            "type": "relay",
            "topic": topic,
            "data": encoded_resp
        }),
    )
    .await
    .unwrap();

    // --- Step 3: D2 receives the sync_response and applies it ---
    let recv_d2 = wait_for_message_type(&mut stream_d2, "relay", 5).await.unwrap();
    let received_bytes_d2 = base64::engine::general_purpose::STANDARD
        .decode(recv_d2["data"].as_str().unwrap())
        .unwrap();
    let received_wire_d2: GossipWireMessage = serde_json::from_slice(&received_bytes_d2).unwrap();
    assert_eq!(received_wire_d2.kind, "sync_response");

    let diff_encrypted = base64::engine::general_purpose::STANDARD
        .decode(&received_wire_d2.payload)
        .unwrap();
    let diff = crypto::decrypt(&group_key, &diff_encrypted).unwrap();

    node_d2
        .doc_manager
        .lock()
        .await
        .apply_update(&format!("group/{group_id}"), &diff)
        .unwrap();

    // D2 now has D1's pin and location.
    let state_d2 = node_d2.group_manager.get_group_state(group_id).await.unwrap();
    assert_eq!(state_d2.pins.len(), 1, "D2 should see D1's pin after SV sync");
    assert_eq!(state_d2.pins[0].label, "Base Camp");
    let d1_member = state_d2
        .members
        .iter()
        .find(|m| m.user_id == "d1-user")
        .expect("D2 should see d1-user after sync");
    assert_eq!(d1_member.stage_id.as_deref(), Some("main-stage"));

    // --- Step 4: Reverse handshake so D1 gets D2's member entry ---
    // D1 sends its SV, D2 computes diff (with member/d2-user), D1 applies.
    let encrypted_sv_d1 = node_d1.group_manager.request_group_sync(group_id).await.unwrap();
    let encoded_sv_d1 = base64::engine::general_purpose::STANDARD.encode(&encrypted_sv_d1);
    send_json(&mut sink_d1, &serde_json::json!({ "type": "relay", "topic": topic, "data": encoded_sv_d1 })).await.unwrap();

    let recv_sv_d2 = wait_for_message_type(&mut stream_d2, "relay", 5).await.unwrap();
    let sv_d1_enc = base64::engine::general_purpose::STANDARD.decode(recv_sv_d2["data"].as_str().unwrap()).unwrap();
    let diff_for_d1 = node_d2.group_manager.handle_sync_request(group_id, &sv_d1_enc).await.unwrap();
    let encoded_diff_d1 = base64::engine::general_purpose::STANDARD.encode(&diff_for_d1);
    send_json(&mut sink_d2, &serde_json::json!({ "type": "relay", "topic": topic, "data": encoded_diff_d1 })).await.unwrap();

    let recv_diff_d1 = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let diff_d1_enc = base64::engine::general_purpose::STANDARD.decode(recv_diff_d1["data"].as_str().unwrap()).unwrap();
    let diff_d1_bytes = crypto::decrypt(&group_key, &diff_d1_enc).unwrap();
    node_d1.doc_manager.lock().await.apply_update(&format!("group/{group_id}"), &diff_d1_bytes).unwrap();

    // --- Step 5: D2 makes a change and sends diff (works now — D1 has D2's state) ---
    let encrypted_d2_diff = node_d2
        .group_manager
        .check_in(group_id, "d2-user", Some("side-stage"), None)
        .await
        .unwrap();
    let encoded_d2_diff = base64::engine::general_purpose::STANDARD.encode(&encrypted_d2_diff);
    send_json(&mut sink_d2, &serde_json::json!({ "type": "relay", "topic": topic, "data": encoded_d2_diff })).await.unwrap();

    // D1 receives and applies D2's diff.
    let recv_d1_update = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let d2_diff_enc = base64::engine::general_purpose::STANDARD.decode(recv_d1_update["data"].as_str().unwrap()).unwrap();
    let d2_diff = crypto::decrypt(&group_key, &d2_diff_enc).unwrap();
    node_d1.doc_manager.lock().await.apply_update(&format!("group/{group_id}"), &d2_diff).unwrap();

    let state_d1 = node_d1.group_manager.get_group_state(group_id).await.unwrap();
    let d2_in_d1 = state_d1
        .members
        .iter()
        .find(|m| m.user_id == "d2-user")
        .expect("D1 should see d2-user after sync");
    assert_eq!(d2_in_d1.stage_id.as_deref(), Some("side-stage"));

    drop(sink_d1);
    drop(sink_d2);
}

// =============================================================================
// Test: Festival stage chat — D1 and D2 subscribe to a stage topic, D1 sends,
//       D2 receives; D1 sends on a different stage that D2 is NOT subscribed to,
//       D2 does NOT receive it.
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_festival_stage_chat_via_relay() {
    let festival_id = "chat-stage-test-1";
    let topic_stage1 = format!("festival/{festival_id}/chat/stage-1");
    let topic_stage2 = format!("festival/{festival_id}/chat/stage-2");

    // D1 subscribes to both stages, D2 only to stage-1.
    let (mut sink_d1, _stream_d1) = connect_and_subscribe(festival_id, &topic_stage1)
        .await
        .unwrap();
    let (sink_d2, mut stream_d2) = connect_and_subscribe(festival_id, &topic_stage1)
        .await
        .unwrap();

    // Also subscribe D1 to stage-2 (D2 is NOT subscribed to stage-2).
    send_json(
        &mut sink_d1,
        &json!({
            "type": "subscribe",
            "topics": [&topic_stage2]
        }),
    )
    .await
    .unwrap();

    // D1 sends chat on stage-1 — D2 should receive it.
    let msg_id_1 = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_d1,
        &json!({
            "type": "chat",
            "topic": topic_stage1,
            "message": {
                "id": msg_id_1,
                "userId": "d1",
                "displayName": "D1",
                "text": "Stage 1 message",
                "topic": topic_stage1,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    let recv = wait_for_message_type(&mut stream_d2, "chat", 5).await.unwrap();
    assert_eq!(recv["message"]["id"], msg_id_1);
    assert_eq!(recv["message"]["text"], "Stage 1 message");

    // D1 sends chat on stage-2 — D2 should NOT receive it within timeout.
    let msg_id_2 = uuid::Uuid::new_v4().to_string();
    send_json(
        &mut sink_d1,
        &json!({
            "type": "chat",
            "topic": topic_stage2,
            "message": {
                "id": msg_id_2,
                "userId": "d1",
                "displayName": "D1",
                "text": "Stage 2 message",
                "topic": topic_stage2,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }
        }),
    )
    .await
    .unwrap();

    // D2 is not subscribed to stage-2 — should time out.
    let result = tokio::time::timeout(
        Duration::from_secs(2),
        wait_for_message_type(&mut stream_d2, "chat", 2),
    )
    .await;
    // Either timeout or a message that is NOT the stage-2 message.
    match result {
        Err(_) => {} // expected timeout
        Ok(Ok(msg)) => {
            // If something came in, make sure it's not the stage-2 message.
            assert_ne!(msg["message"]["id"].as_str(), Some(msg_id_2.as_str()));
        }
        Ok(Err(_)) => {} // connection error — acceptable
    }

    drop(sink_d1);
    drop(sink_d2);
}

// =============================================================================
// Test: Encrypted group chat via relay — D1 sends, D2 (with shared key)
//       receives and decrypts; D3 (no key) receives ciphertext but cannot decrypt.
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_encrypted_group_chat_via_relay() {
    use base64::Engine as _;
    use offbeat_core::crypto;
    use offbeat_core::gossip_manager::GossipWireMessage;
    use offbeat_core::types::ChatMessage;

    let festival_id = "chat-group-test-1";
    let group_key = crypto::generate_group_key();
    let group_id = crypto::group_id_from_key(&group_key);
    let topic = format!("group/{group_id}/chat");

    // Build and encrypt a chat message.
    let original = ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: "d1-user".to_string(),
        display_name: "D1".to_string(),
        text: "secret group hello".to_string(),
        topic: topic.clone(),
        stage_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
    };
    let plaintext = serde_json::to_vec(&original).unwrap();
    let encrypted = crypto::encrypt(&group_key, &plaintext).unwrap();

    // Wrap in a GossipWireMessage.
    let wire = GossipWireMessage {
        kind: "encrypted_chat".to_string(),
        doc_id: None,
        payload: base64::engine::general_purpose::STANDARD.encode(&encrypted),
        group_key_id: Some(crypto::group_id_from_key(&group_key)),
    };
    let wire_bytes = serde_json::to_vec(&wire).unwrap();
    let encoded_relay = base64::engine::general_purpose::STANDARD.encode(&wire_bytes);

    // D1 sends, D2 receives.
    let (mut sink_d1, _) = connect_and_subscribe(festival_id, &topic).await.unwrap();
    let (_sink_d2, mut stream_d2) = connect_and_subscribe(festival_id, &topic).await.unwrap();
    // D3: eavesdropper — subscribes but has no key.
    let (_sink_d3, mut stream_d3) = connect_and_subscribe(festival_id, &topic).await.unwrap();

    send_json(
        &mut sink_d1,
        &json!({
            "type": "relay",
            "topic": topic,
            "data": encoded_relay
        }),
    )
    .await
    .unwrap();

    // D2 receives the relay payload.
    let recv_d2 = wait_for_message_type(&mut stream_d2, "relay", 5).await.unwrap();
    let received_bytes = base64::engine::general_purpose::STANDARD
        .decode(recv_d2["data"].as_str().unwrap())
        .unwrap();
    let received_wire: GossipWireMessage = serde_json::from_slice(&received_bytes).unwrap();
    assert_eq!(received_wire.kind, "encrypted_chat");

    // D2 can decrypt and verify the message.
    let enc = base64::engine::general_purpose::STANDARD
        .decode(&received_wire.payload)
        .unwrap();
    let pt = crypto::decrypt(&group_key, &enc).unwrap();
    let msg: ChatMessage = serde_json::from_slice(&pt).unwrap();
    assert_eq!(msg.text, "secret group hello");
    assert_eq!(msg.id, original.id);

    // D3 also receives the relay (DO relays blindly), but decryption fails.
    let recv_d3 = wait_for_message_type(&mut stream_d3, "relay", 5).await.unwrap();
    let recv_bytes_d3 = base64::engine::general_purpose::STANDARD
        .decode(recv_d3["data"].as_str().unwrap())
        .unwrap();
    let wire_d3: GossipWireMessage = serde_json::from_slice(&recv_bytes_d3).unwrap();
    assert_eq!(wire_d3.kind, "encrypted_chat");

    let wrong_key = crypto::generate_group_key(); // D3 has no real key
    let enc_d3 = base64::engine::general_purpose::STANDARD
        .decode(&wire_d3.payload)
        .unwrap();
    let decrypt_result = crypto::decrypt(&wrong_key, &enc_d3);
    assert!(decrypt_result.is_err(), "D3 should not be able to decrypt");

    drop(sink_d1);
}

// =============================================================================
// Test: Chat catch-up on reconnect — D1 sends 5 messages, D2 connects late,
//       requests catchup, receives all 5.
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_chat_catchup_on_reconnect() {
    let festival_id = "chat-catchup-test-1";
    let topic = format!("festival/{festival_id}/chat/general");

    // D1 connects and sends 5 messages.
    let (mut sink_d1, _stream_d1) = connect_and_subscribe(festival_id, &topic).await.unwrap();

    let mut msg_ids = Vec::new();
    for i in 1..=5 {
        let msg_id = uuid::Uuid::new_v4().to_string();
        msg_ids.push(msg_id.clone());
        send_json(
            &mut sink_d1,
            &json!({
                "type": "chat",
                "topic": topic,
                "message": {
                    "id": msg_id,
                    "userId": "d1",
                    "displayName": "D1",
                    "text": format!("Catchup message {i}"),
                    "topic": topic,
                    "timestamp": chrono::Utc::now().to_rfc3339()
                }
            }),
        )
        .await
        .unwrap();
    }

    // Wait for messages to be stored.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // D2 connects late and requests a full catchup.
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(festival_id, &topic).await.unwrap();

    send_json(
        &mut sink_d2,
        &json!({
            "type": "catchup",
            "topic": topic,
            "sinceSeq": 0
        }),
    )
    .await
    .unwrap();

    let catchup = wait_for_message_type(&mut stream_d2, "catchup", 5).await.unwrap();
    assert_eq!(catchup["type"], "catchup");

    let chat_msgs = catchup["chat"].as_array().unwrap();

    // All 5 messages must be present.
    for msg_id in &msg_ids {
        let found = chat_msgs
            .iter()
            .any(|m| m["message"]["id"].as_str() == Some(msg_id.as_str()));
        assert!(found, "message {msg_id} not found in catchup");
    }

    drop(sink_d1);
    drop(sink_d2);
}

