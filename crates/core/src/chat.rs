//! Chat — sending and receiving chat over gossip and WS relay.

use std::sync::Arc;

use iroh_gossip::proto::TopicId;

use crate::crypto;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::topics;
use crate::types::ChatMessage;

// ---------------------------------------------------------------------------
// Sending
// ---------------------------------------------------------------------------

/// Create and locally persist a festival chat message.
///
/// Returns `(message, topic_id)` — the caller is responsible for
/// broadcasting over gossip / relay.
pub fn send_festival_chat(
    db: &Database,
    festival_id: &str,
    stage_id: Option<&str>,
    user_id: &str,
    display_name: &str,
    text: &str,
) -> anyhow::Result<(ChatMessage, TopicId)> {
    let stage_or_general = stage_id.unwrap_or("general");
    let topic = format!("festival/{festival_id}/chat/{stage_or_general}");

    let msg = db.save_local_chat_message(ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        display_name: display_name.to_string(),
        text: text.to_string(),
        topic: topic.clone(),
        stage_id: stage_id.map(ToOwned::to_owned),
        timestamp: now_rfc3339(),
        writer_seq: 0,
        logical_time: 0,
    })?;

    let topic_id = topics::festival_topic(festival_id, &format!("chat/{stage_or_general}"));

    Ok((msg, topic_id))
}

/// Create, encrypt, and locally persist a group chat message.
///
/// Returns `(encrypted_bytes, topic_id)` — the caller broadcasts
/// the raw encrypted bytes over gossip / relay.
pub fn send_group_chat(
    db: &Database,
    group_id: &str,
    user_id: &str,
    display_name: &str,
    text: &str,
) -> anyhow::Result<(Vec<u8>, TopicId)> {
    let topic = format!("group/{group_id}/chat");

    let msg = db.save_local_chat_message(ChatMessage {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user_id.to_string(),
        display_name: display_name.to_string(),
        text: text.to_string(),
        topic: topic.clone(),
        stage_id: None,
        timestamp: now_rfc3339(),
        writer_seq: 0,
        logical_time: 0,
    })?;

    let group_key = db
        .load_group_key(group_id)?
        .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

    let plaintext = serde_json::to_vec(&msg)?;
    let encrypted = crypto::encrypt(&group_key, &plaintext)?;

    let topic_id = topics::group_topic(&group_key, "chat");

    Ok((encrypted, topic_id))
}

// ---------------------------------------------------------------------------
// Receiving
// ---------------------------------------------------------------------------

/// Persist an incoming plaintext festival chat message (dedup by ID).
pub fn receive_festival_chat(db: &Database, message: ChatMessage) -> anyhow::Result<()> {
    db.save_chat_message(&message)
}

/// Decrypt an incoming group chat message, persist it, and return it.
pub fn receive_encrypted_group_chat(
    db: &Database,
    group_id: &str,
    encrypted: &[u8],
) -> anyhow::Result<ChatMessage> {
    let group_key = db
        .load_group_key(group_id)?
        .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

    let plaintext = crypto::decrypt(&group_key, encrypted)?;
    let message: ChatMessage = serde_json::from_slice(&plaintext)
        .map_err(|e| anyhow::anyhow!("deserialise group chat: {e}"))?;

    db.save_chat_message(&message)?;

    Ok(message)
}

// ---------------------------------------------------------------------------
// Query
// ---------------------------------------------------------------------------

/// Return paginated chat history for a topic string.
pub fn get_history(
    db: &Database,
    topic: &str,
    limit: u32,
    offset: u32,
) -> anyhow::Result<Vec<ChatMessage>> {
    db.get_chat_messages(topic, limit, offset)
}

/// Return `(topic_string, TopicId)` pairs for general, campsite, and each
/// stage channel of a festival.
pub fn get_festival_chat_topics(festival_id: &str, stage_ids: &[&str]) -> Vec<(String, TopicId)> {
    let mut result = Vec::new();

    result.push((
        format!("festival/{festival_id}/chat/general"),
        topics::festival_topic(festival_id, "chat/general"),
    ));

    result.push((
        format!("festival/{festival_id}/chat/campsite"),
        topics::festival_topic(festival_id, "chat/campsite"),
    ));

    for stage_id in stage_ids {
        result.push((
            format!("festival/{festival_id}/chat/{stage_id}"),
            topics::festival_topic(festival_id, &format!("chat/{stage_id}")),
        ));
    }

    result
}

// ---------------------------------------------------------------------------
// Legacy ChatManager (wraps free functions for backward compat)
// ---------------------------------------------------------------------------

/// Thin wrapper around the free functions for callers that still hold `Arc<ChatManager>`.
pub struct ChatManager {
    db: Arc<Database>,
    #[allow(dead_code)]
    doc_manager: Arc<DocManager>,
}

impl ChatManager {
    pub fn new(db: Arc<Database>, doc_manager: Arc<DocManager>) -> Self {
        Self { db, doc_manager }
    }

    pub fn send_festival_chat(
        &self,
        festival_id: &str,
        stage_id: Option<&str>,
        user_id: &str,
        display_name: &str,
        text: &str,
    ) -> anyhow::Result<(ChatMessage, TopicId)> {
        send_festival_chat(&self.db, festival_id, stage_id, user_id, display_name, text)
    }

    pub fn send_group_chat(
        &self,
        group_id: &str,
        user_id: &str,
        display_name: &str,
        text: &str,
    ) -> anyhow::Result<(Vec<u8>, TopicId)> {
        send_group_chat(&self.db, group_id, user_id, display_name, text)
    }

    pub fn receive_festival_chat(&self, message: ChatMessage) -> anyhow::Result<()> {
        receive_festival_chat(&self.db, message)
    }

    pub fn receive_encrypted_group_chat(
        &self,
        group_id: &str,
        encrypted: &[u8],
    ) -> anyhow::Result<ChatMessage> {
        receive_encrypted_group_chat(&self.db, group_id, encrypted)
    }

    pub fn get_history(
        &self,
        topic: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        get_history(&self.db, topic, limit, offset)
    }

    pub fn get_festival_chat_topics(
        &self,
        festival_id: &str,
        stage_ids: &[&str],
    ) -> Vec<(String, TopicId)> {
        get_festival_chat_topics(festival_id, stage_ids)
    }
}

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}Z")
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    #[test]
    fn test_send_festival_chat_general() {
        let db = test_db();
        let (msg, topic_id) =
            send_festival_chat(&db, "fieldday", None, "user1", "Alice", "hello").unwrap();

        assert_eq!(msg.topic, "festival/fieldday/chat/general");
        assert_eq!(msg.stage_id, None);
        assert_eq!(msg.user_id, "user1");
        assert_eq!(msg.text, "hello");
        assert_eq!((msg.writer_seq, msg.logical_time), (1, 1));

        let expected_id = topics::festival_topic("fieldday", "chat/general");
        assert_eq!(topic_id, expected_id);

        let stored = db
            .get_chat_messages("festival/fieldday/chat/general", 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
    }

    #[test]
    fn test_send_festival_chat_stage() {
        let db = test_db();
        let (msg, topic_id) = send_festival_chat(
            &db,
            "fieldday",
            Some("main-stage"),
            "user1",
            "Alice",
            "nice set!",
        )
        .unwrap();

        assert_eq!(msg.topic, "festival/fieldday/chat/main-stage");
        assert_eq!(msg.stage_id.as_deref(), Some("main-stage"));

        let expected_id = topics::festival_topic("fieldday", "chat/main-stage");
        assert_eq!(topic_id, expected_id);
    }

    #[test]
    fn test_send_group_chat() {
        let db = test_db();
        let group_key = crypto::generate_group_key();
        let group_id = crypto::group_id_from_key(&group_key);
        db.save_group(&group_id, "fest-1", "Test Group", &group_key)
            .unwrap();

        let (encrypted, topic_id) =
            send_group_chat(&db, &group_id, "user1", "Alice", "secret msg").unwrap();

        assert!(!encrypted.is_empty());

        let expected_id = topics::group_topic(&group_key, "chat");
        assert_eq!(topic_id, expected_id);

        let stored = db
            .get_chat_messages(&format!("group/{group_id}/chat"), 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text, "secret msg");
        assert_eq!((stored[0].writer_seq, stored[0].logical_time), (1, 1));

        let plaintext = crypto::decrypt(&group_key, &encrypted).unwrap();
        let decoded: ChatMessage = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(decoded.text, "secret msg");
    }

    #[test]
    fn test_receive_festival_chat_saves() {
        let db = test_db();
        let msg = ChatMessage {
            id: "m1".to_string(),
            user_id: "u2".to_string(),
            display_name: "Bob".to_string(),
            text: "hi".to_string(),
            topic: "festival/fieldday/chat/general".to_string(),
            stage_id: None,
            timestamp: "2026-06-14T20:00:00Z".to_string(),
            writer_seq: 0,
            logical_time: 0,
        };
        receive_festival_chat(&db, msg).unwrap();

        let stored = db
            .get_chat_messages("festival/fieldday/chat/general", 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text, "hi");
    }

    #[test]
    fn test_festival_chat_topics() {
        let topics = get_festival_chat_topics("fieldday", &["main-stage", "second-stage"]);
        assert_eq!(topics.len(), 4);

        let topic_strings: Vec<&str> = topics.iter().map(|(s, _)| s.as_str()).collect();
        assert!(topic_strings.contains(&"festival/fieldday/chat/general"));
        assert!(topic_strings.contains(&"festival/fieldday/chat/campsite"));
        assert!(topic_strings.contains(&"festival/fieldday/chat/main-stage"));
        assert!(topic_strings.contains(&"festival/fieldday/chat/second-stage"));
    }

    #[test]
    fn test_get_history_pagination() {
        let db = test_db();
        let topic = "festival/fieldday/chat/general";

        for i in 0..5u32 {
            receive_festival_chat(
                &db,
                ChatMessage {
                    id: format!("h{i}"),
                    user_id: "u1".to_string(),
                    display_name: "Alice".to_string(),
                    text: format!("message {i}"),
                    topic: topic.to_string(),
                    stage_id: None,
                    timestamp: format!("2026-06-14T20:0{i}:00Z"),
                    writer_seq: i as u64,
                    logical_time: i as u64,
                },
            )
            .unwrap();
        }

        let page1 = get_history(&db, topic, 3, 0).unwrap();
        assert_eq!(
            page1
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["h0", "h1", "h2"]
        );

        let page2 = get_history(&db, topic, 3, 3).unwrap();
        assert_eq!(
            page2
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["h3", "h4"]
        );
    }
}
