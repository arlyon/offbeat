//! ChatManager — orchestrates sending and receiving chat over gossip and WS relay.

use std::sync::Arc;

use iroh_gossip::proto::TopicId;
use tokio::sync::Mutex;

use crate::crypto;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::topics;
use crate::types::ChatMessage;

// ---------------------------------------------------------------------------
// ChatManager
// ---------------------------------------------------------------------------

pub struct ChatManager {
    db: Arc<Database>,
    /// Held for potential future use (group state access, etc.).
    #[allow(dead_code)]
    doc_manager: Arc<Mutex<DocManager>>,
}

impl ChatManager {
    pub fn new(db: Arc<Database>, doc_manager: Arc<Mutex<DocManager>>) -> Self {
        Self { db, doc_manager }
    }

    // -----------------------------------------------------------------------
    // Sending
    // -----------------------------------------------------------------------

    /// Create and locally persist a festival chat message.
    ///
    /// Returns `(message, topic_id)` — the caller is responsible for
    /// broadcasting over gossip / relay.
    pub fn send_festival_chat(
        &self,
        festival_id: &str,
        stage_id: Option<&str>,
        user_id: &str,
        display_name: &str,
        text: &str,
    ) -> anyhow::Result<(ChatMessage, TopicId)> {
        let stage_or_general = stage_id.unwrap_or("general");
        let topic = format!("festival/{festival_id}/chat/{stage_or_general}");

        let msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            display_name: display_name.to_string(),
            text: text.to_string(),
            topic: topic.clone(),
            stage_id: stage_id.map(ToOwned::to_owned),
            timestamp: now_rfc3339(),
        };

        self.db.save_chat_message(&msg)?;

        let topic_id = topics::festival_topic(festival_id, &format!("chat/{stage_or_general}"));

        Ok((msg, topic_id))
    }

    /// Create, encrypt, and locally persist a group chat message.
    ///
    /// Returns `(encrypted_bytes, topic_id)` — the caller broadcasts
    /// the raw encrypted bytes over gossip / relay.
    pub fn send_group_chat(
        &self,
        group_id: &str,
        user_id: &str,
        display_name: &str,
        text: &str,
    ) -> anyhow::Result<(Vec<u8>, TopicId)> {
        let topic = format!("group/{group_id}/chat");

        let msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            display_name: display_name.to_string(),
            text: text.to_string(),
            topic: topic.clone(),
            stage_id: None,
            timestamp: now_rfc3339(),
        };

        self.db.save_chat_message(&msg)?;

        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let plaintext = serde_json::to_vec(&msg)?;
        let encrypted = crypto::encrypt(&group_key, &plaintext)?;

        let topic_id = topics::group_topic(&group_key, "chat");

        Ok((encrypted, topic_id))
    }

    // -----------------------------------------------------------------------
    // Receiving
    // -----------------------------------------------------------------------

    /// Persist an incoming plaintext festival chat message (dedup by ID).
    pub fn receive_festival_chat(&self, message: ChatMessage) -> anyhow::Result<()> {
        // INSERT OR REPLACE in save_chat_message handles deduplication.
        self.db.save_chat_message(&message)
    }

    /// Decrypt an incoming group chat message, persist it, and return it.
    pub fn receive_encrypted_group_chat(
        &self,
        group_id: &str,
        encrypted: &[u8],
    ) -> anyhow::Result<ChatMessage> {
        let group_key = self
            .db
            .load_group_key(group_id)?
            .ok_or_else(|| anyhow::anyhow!("group not found: {group_id}"))?;

        let plaintext = crypto::decrypt(&group_key, encrypted)?;
        let message: ChatMessage = serde_json::from_slice(&plaintext)
            .map_err(|e| anyhow::anyhow!("deserialise group chat: {e}"))?;

        self.db.save_chat_message(&message)?;

        Ok(message)
    }

    // -----------------------------------------------------------------------
    // Query
    // -----------------------------------------------------------------------

    /// Return paginated chat history for a topic string.
    pub fn get_history(
        &self,
        topic: &str,
        limit: u32,
        offset: u32,
    ) -> anyhow::Result<Vec<ChatMessage>> {
        self.db.get_chat_messages(topic, limit, offset)
    }

    /// Return `(topic_string, TopicId)` pairs for general, campsite, and each
    /// stage channel of a festival.
    pub fn get_festival_chat_topics(
        &self,
        festival_id: &str,
        stage_ids: &[&str],
    ) -> Vec<(String, TopicId)> {
        let mut result = Vec::new();

        // General chat
        result.push((
            format!("festival/{festival_id}/chat/general"),
            topics::festival_topic(festival_id, "chat/general"),
        ));

        // Campsite chat
        result.push((
            format!("festival/{festival_id}/chat/campsite"),
            topics::festival_topic(festival_id, "chat/campsite"),
        ));

        // Per-stage chats
        for stage_id in stage_ids {
            result.push((
                format!("festival/{festival_id}/chat/{stage_id}"),
                topics::festival_topic(festival_id, &format!("chat/{stage_id}")),
            ));
        }

        result
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

    fn make_manager() -> ChatManager {
        let db = Arc::new(Database::new_in_memory().expect("in-memory db"));
        let doc_manager = Arc::new(Mutex::new(crate::doc_manager::DocManager::new(db.clone())));
        ChatManager::new(db, doc_manager)
    }

    fn make_manager_with_group() -> (ChatManager, String, [u8; 32]) {
        let db = Arc::new(Database::new_in_memory().expect("in-memory db"));
        let doc_manager = Arc::new(Mutex::new(crate::doc_manager::DocManager::new(db.clone())));

        let group_key = crypto::generate_group_key();
        let group_id = crypto::group_id_from_key(&group_key);
        db.save_group(&group_id, "fest-1", "Test Group", &group_key)
            .expect("save_group");

        let mgr = ChatManager::new(db, doc_manager);
        (mgr, group_id, group_key)
    }

    // -----------------------------------------------------------------------
    // send_festival_chat
    // -----------------------------------------------------------------------

    #[test]
    fn test_send_festival_chat_general() {
        let mgr = make_manager();
        let (msg, topic_id) = mgr
            .send_festival_chat("fieldday", None, "user1", "Alice", "hello")
            .unwrap();

        assert_eq!(msg.topic, "festival/fieldday/chat/general");
        assert_eq!(msg.stage_id, None);
        assert_eq!(msg.user_id, "user1");
        assert_eq!(msg.text, "hello");
        assert!(!msg.id.is_empty());

        // Topic ID should be deterministic.
        let expected_id = topics::festival_topic("fieldday", "chat/general");
        assert_eq!(topic_id, expected_id);

        // Message should be persisted.
        let stored = mgr
            .db
            .get_chat_messages("festival/fieldday/chat/general", 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, msg.id);
    }

    #[test]
    fn test_send_festival_chat_stage() {
        let mgr = make_manager();
        let (msg, topic_id) = mgr
            .send_festival_chat("fieldday", Some("main-stage"), "user1", "Alice", "nice set!")
            .unwrap();

        assert_eq!(msg.topic, "festival/fieldday/chat/main-stage");
        assert_eq!(msg.stage_id.as_deref(), Some("main-stage"));

        let expected_id = topics::festival_topic("fieldday", "chat/main-stage");
        assert_eq!(topic_id, expected_id);
    }

    // -----------------------------------------------------------------------
    // send_group_chat
    // -----------------------------------------------------------------------

    #[test]
    fn test_send_group_chat() {
        let (mgr, group_id, group_key) = make_manager_with_group();
        let (encrypted, topic_id) = mgr
            .send_group_chat(&group_id, "user1", "Alice", "secret msg")
            .unwrap();

        // Should be non-empty ciphertext.
        assert!(!encrypted.is_empty());

        // Topic ID should be derived from the group key.
        let expected_id = topics::group_topic(&group_key, "chat");
        assert_eq!(topic_id, expected_id);

        // Local message should be persisted (plaintext in DB).
        let stored = mgr
            .db
            .get_chat_messages(&format!("group/{group_id}/chat"), 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text, "secret msg");

        // Ciphertext round-trips.
        let plaintext = crypto::decrypt(&group_key, &encrypted).unwrap();
        let decoded: ChatMessage = serde_json::from_slice(&plaintext).unwrap();
        assert_eq!(decoded.text, "secret msg");
    }

    // -----------------------------------------------------------------------
    // receive_festival_chat
    // -----------------------------------------------------------------------

    #[test]
    fn test_receive_festival_chat_saves() {
        let mgr = make_manager();
        let msg = ChatMessage {
            id: "m1".to_string(),
            user_id: "u2".to_string(),
            display_name: "Bob".to_string(),
            text: "hi".to_string(),
            topic: "festival/fieldday/chat/general".to_string(),
            stage_id: None,
            timestamp: "2026-06-14T20:00:00Z".to_string(),
        };
        mgr.receive_festival_chat(msg).unwrap();

        let stored = mgr
            .db
            .get_chat_messages("festival/fieldday/chat/general", 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text, "hi");
    }

    #[test]
    fn test_receive_festival_chat_deduplication() {
        let mgr = make_manager();
        let msg = ChatMessage {
            id: "dup1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "duplicate".to_string(),
            topic: "festival/fieldday/chat/general".to_string(),
            stage_id: None,
            timestamp: "2026-06-14T20:00:00Z".to_string(),
        };
        mgr.receive_festival_chat(msg.clone()).unwrap();
        mgr.receive_festival_chat(msg).unwrap(); // second insert — should replace, not duplicate

        let stored = mgr
            .db
            .get_chat_messages("festival/fieldday/chat/general", 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1, "duplicate should not create a second row");
    }

    // -----------------------------------------------------------------------
    // receive_encrypted_group_chat
    // -----------------------------------------------------------------------

    #[test]
    fn test_receive_encrypted_group_chat() {
        let (mgr, group_id, group_key) = make_manager_with_group();

        let original = ChatMessage {
            id: "gc1".to_string(),
            user_id: "u3".to_string(),
            display_name: "Carol".to_string(),
            text: "group hello".to_string(),
            topic: format!("group/{group_id}/chat"),
            stage_id: None,
            timestamp: "2026-06-14T21:00:00Z".to_string(),
        };

        let plaintext = serde_json::to_vec(&original).unwrap();
        let encrypted = crypto::encrypt(&group_key, &plaintext).unwrap();

        let returned = mgr
            .receive_encrypted_group_chat(&group_id, &encrypted)
            .unwrap();

        assert_eq!(returned.id, "gc1");
        assert_eq!(returned.text, "group hello");

        let stored = mgr
            .db
            .get_chat_messages(&format!("group/{group_id}/chat"), 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, "gc1");
    }

    // -----------------------------------------------------------------------
    // get_history
    // -----------------------------------------------------------------------

    #[test]
    fn test_get_history_pagination() {
        let mgr = make_manager();
        let topic = "festival/fieldday/chat/general";

        for i in 0..5u32 {
            mgr.receive_festival_chat(ChatMessage {
                id: format!("h{i}"),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: format!("message {i}"),
                topic: topic.to_string(),
                stage_id: None,
                // Use ascending timestamps so ordering is deterministic.
                timestamp: format!("2026-06-14T20:0{i}:00Z"),
            })
            .unwrap();
        }

        // Limit
        let page1 = mgr.get_history(topic, 3, 0).unwrap();
        assert_eq!(page1.len(), 3);

        // Offset
        let page2 = mgr.get_history(topic, 3, 3).unwrap();
        assert_eq!(page2.len(), 2);

        // No overlap
        let ids1: Vec<_> = page1.iter().map(|m| &m.id).collect();
        let ids2: Vec<_> = page2.iter().map(|m| &m.id).collect();
        for id in &ids2 {
            assert!(!ids1.contains(id), "pages should not overlap");
        }
    }

    // -----------------------------------------------------------------------
    // get_festival_chat_topics
    // -----------------------------------------------------------------------

    #[test]
    fn test_festival_chat_topics() {
        let mgr = make_manager();
        let topics = mgr.get_festival_chat_topics("fieldday", &["main-stage", "second-stage"]);

        // Expect: general, campsite, main-stage, second-stage = 4 topics
        assert_eq!(topics.len(), 4);

        let topic_strings: Vec<&str> = topics.iter().map(|(s, _)| s.as_str()).collect();
        assert!(topic_strings.contains(&"festival/fieldday/chat/general"));
        assert!(topic_strings.contains(&"festival/fieldday/chat/campsite"));
        assert!(topic_strings.contains(&"festival/fieldday/chat/main-stage"));
        assert!(topic_strings.contains(&"festival/fieldday/chat/second-stage"));

        // TopicIds should be deterministic.
        let topics2 = mgr.get_festival_chat_topics("fieldday", &["main-stage", "second-stage"]);
        for (i, (s, id)) in topics.iter().enumerate() {
            assert_eq!(*s, topics2[i].0);
            assert_eq!(*id, topics2[i].1);
        }
    }
}
