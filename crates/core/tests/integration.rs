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
// Test: Group encrypted state sync via DO relay
// =============================================================================

#[tokio::test]
#[ignore = "requires running DO server"]
async fn test_group_encrypted_state_sync_via_relay() {
    use base64::Engine as _;
    use offbeat_core::{OffbeatNode, crypto};

    let festival_id = "relay-group-test-1";
    let topic = format!("festival/{festival_id}/state");

    // D1 creates a group and a shared key via GroupManager.
    let node_d1 = OffbeatNode::new_in_memory().unwrap();
    let create_result = node_d1
        .group_manager
        .create_group(festival_id, "Test Crew", "d1-user", "D1 User")
        .await
        .unwrap();

    let group_id = &create_result.group_id;
    let group_key = node_d1.db.load_group_key(group_id).unwrap().unwrap();

    // D2 joins the group using the invite payload (shares the same key).
    let node_d2 = OffbeatNode::new_in_memory().unwrap();
    node_d2
        .group_manager
        .join_group(&create_result.invite_payload, "d2-user", "D2 User")
        .await
        .unwrap();

    // Connect both to the DO relay.
    let (mut sink_d1, mut stream_d1) = connect_and_subscribe(festival_id, &topic).await.unwrap();
    let (mut sink_d2, mut stream_d2) = connect_and_subscribe(festival_id, &topic).await.unwrap();

    // D1 adds a pin, encrypts the update.
    let encrypted_d1 = node_d1
        .group_manager
        .add_pin(group_id, "pin-relay-1", "Tent Area", "51.5,-0.1", "d1-user")
        .await
        .unwrap();

    // D1 sends the encrypted update as a relay message.
    let encoded = base64::engine::general_purpose::STANDARD.encode(&encrypted_d1);
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

    // D2 receives and applies the update.
    let recv = wait_for_message_type(&mut stream_d2, "relay", 5).await.unwrap();
    let received_encoded = recv["data"].as_str().unwrap();
    let received_encrypted = base64::engine::general_purpose::STANDARD
        .decode(received_encoded)
        .unwrap();

    let decrypted = crypto::decrypt(&group_key, &received_encrypted).unwrap();
    node_d2
        .doc_manager
        .lock()
        .await
        .apply_update(&format!("group/{group_id}"), &decrypted)
        .unwrap();

    // Verify D2 sees the pin.
    let state_d2 = node_d2.group_manager.get_group_state(group_id).await.unwrap();
    assert_eq!(state_d2.pins.len(), 1);
    assert_eq!(state_d2.pins[0].label, "Tent Area");

    // D2 makes a change (check-in), sends back.
    let encrypted_d2 = node_d2
        .group_manager
        .check_in(group_id, "d2-user", Some("main-stage"), None)
        .await
        .unwrap();

    let encoded_d2 = base64::engine::general_purpose::STANDARD.encode(&encrypted_d2);
    send_json(
        &mut sink_d2,
        &json!({
            "type": "relay",
            "topic": topic,
            "data": encoded_d2
        }),
    )
    .await
    .unwrap();

    // D1 receives and applies.
    let recv_d1 = wait_for_message_type(&mut stream_d1, "relay", 5).await.unwrap();
    let received_encoded_d1 = recv_d1["data"].as_str().unwrap();
    let received_encrypted_d1 = base64::engine::general_purpose::STANDARD
        .decode(received_encoded_d1)
        .unwrap();

    let decrypted_d1 = crypto::decrypt(&group_key, &received_encrypted_d1).unwrap();
    node_d1
        .doc_manager
        .lock()
        .await
        .apply_update(&format!("group/{group_id}"), &decrypted_d1)
        .unwrap();

    // D1 sees D2's location.
    let state_d1 = node_d1.group_manager.get_group_state(group_id).await.unwrap();
    let d2_member = state_d1
        .members
        .iter()
        .find(|m| m.user_id == "d2-user")
        .expect("d2-user should be in state");
    assert_eq!(d2_member.stage_id.as_deref(), Some("main-stage"));

    drop(sink_d1);
    drop(sink_d2);
}
