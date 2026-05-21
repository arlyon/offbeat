use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

use crate::types::ChatMessage;

const SCHEMA: &str = include_str!("schema.sql");

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) a database at the given path and run the schema.
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    /// Create an in-memory database (for tests).
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    // --- docs ---

    pub fn save_doc(&self, id: &str, doc_type: &str, data: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO docs (id, doc_type, data, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![id, doc_type, data],
        )?;
        Ok(())
    }

    pub fn load_doc(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT data FROM docs WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Returns the IDs of all docs of the given type.
    pub fn list_docs(&self, doc_type: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM docs WHERE doc_type = ?1 ORDER BY updated_at DESC")?;
        let ids = stmt
            .query_map(params![doc_type], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    // --- groups ---

    pub fn save_group(&self, id: &str, festival_id: &str, name: &str, key: &[u8]) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO groups (id, festival_id, name, key, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            params![id, festival_id, name, key],
        )?;
        Ok(())
    }

    /// Returns (id, name, key) tuples for all groups of the given festival.
    pub fn load_groups(&self, festival_id: &str) -> Result<Vec<(String, String, Vec<u8>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, key FROM groups WHERE festival_id = ?1")?;
        let groups = stmt
            .query_map(params![festival_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(groups)
    }

    pub fn delete_group(&self, id: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- starred sets ---

    /// Toggle a star on a set. Returns the new starred state (true = now starred).
    pub fn toggle_star(&self, festival_id: &str, set_id: &str) -> Result<bool> {
        let exists: bool = self.conn.query_row(
            "SELECT COUNT(*) FROM starred_sets WHERE festival_id = ?1 AND set_id = ?2",
            params![festival_id, set_id],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if exists {
            self.conn.execute(
                "DELETE FROM starred_sets WHERE festival_id = ?1 AND set_id = ?2",
                params![festival_id, set_id],
            )?;
            Ok(false)
        } else {
            self.conn.execute(
                "INSERT INTO starred_sets (festival_id, set_id, starred_at)
                 VALUES (?1, ?2, datetime('now'))",
                params![festival_id, set_id],
            )?;
            Ok(true)
        }
    }

    /// Returns the set IDs that are starred for a festival.
    pub fn get_stars(&self, festival_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT set_id FROM starred_sets WHERE festival_id = ?1")?;
        let ids = stmt
            .query_map(params![festival_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    // --- chat messages ---

    pub fn save_chat_message(&self, msg: &ChatMessage) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO chat_messages
             (id, topic, user_id, display_name, text, stage_id, timestamp, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
            params![
                msg.id,
                msg.topic,
                msg.user_id,
                msg.display_name,
                msg.text,
                msg.stage_id,
                msg.timestamp,
            ],
        )?;
        Ok(())
    }

    pub fn get_chat_messages(
        &self,
        topic: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, user_id, display_name, text, topic, stage_id, timestamp
             FROM chat_messages
             WHERE topic = ?1
             ORDER BY timestamp ASC
             LIMIT ?2 OFFSET ?3",
        )?;
        let msgs = stmt
            .query_map(params![topic, limit, offset], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    display_name: row.get(2)?,
                    text: row.get(3)?,
                    topic: row.get(4)?,
                    stage_id: row.get(5)?,
                    timestamp: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(msgs)
    }

    // --- gossip log ---

    /// Save a gossip entry and return the assigned sequence number.
    pub fn save_gossip(&self, topic: &str, data: &[u8]) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO gossip_log (topic, data, received_at)
             VALUES (?1, ?2, datetime('now'))",
            params![topic, data],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Return all gossip entries for a topic with seq > since_seq.
    pub fn get_gossip_since(&self, topic: &str, since_seq: i64) -> Result<Vec<(i64, Vec<u8>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, data FROM gossip_log
             WHERE topic = ?1 AND seq > ?2
             ORDER BY seq ASC",
        )?;
        let entries = stmt
            .query_map(params![topic, since_seq], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChatMessage;

    fn test_db() -> Database {
        Database::new_in_memory().expect("in-memory db")
    }

    #[test]
    fn test_save_and_load_doc() {
        let db = test_db();
        let data = b"hello world";
        db.save_doc("doc1", "festival", data).unwrap();
        let loaded = db.load_doc("doc1").unwrap();
        assert_eq!(loaded, Some(data.to_vec()));
    }

    #[test]
    fn test_load_doc_missing() {
        let db = test_db();
        let loaded = db.load_doc("nonexistent").unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn test_list_docs() {
        let db = test_db();
        db.save_doc("a", "festival", b"data_a").unwrap();
        db.save_doc("b", "festival", b"data_b").unwrap();
        db.save_doc("c", "group", b"data_c").unwrap();
        let ids = db.list_docs("festival").unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"a".to_string()));
        assert!(ids.contains(&"b".to_string()));
    }

    #[test]
    fn test_save_and_load_group() {
        let db = test_db();
        let key = vec![0u8; 32];
        db.save_group("g1", "f1", "My Group", &key).unwrap();
        let groups = db.load_groups("f1").unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "g1");
        assert_eq!(groups[0].1, "My Group");
        assert_eq!(groups[0].2, key);
    }

    #[test]
    fn test_delete_group() {
        let db = test_db();
        db.save_group("g1", "f1", "My Group", &[0u8; 32]).unwrap();
        db.delete_group("g1").unwrap();
        let groups = db.load_groups("f1").unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn test_toggle_star() {
        let db = test_db();
        // First toggle → starred
        let starred = db.toggle_star("f1", "s1").unwrap();
        assert!(starred);
        // Second toggle → unstarred
        let starred = db.toggle_star("f1", "s1").unwrap();
        assert!(!starred);
    }

    #[test]
    fn test_get_stars() {
        let db = test_db();
        db.toggle_star("f1", "s1").unwrap();
        db.toggle_star("f1", "s2").unwrap();
        let stars = db.get_stars("f1").unwrap();
        assert_eq!(stars.len(), 2);
    }

    #[test]
    fn test_save_and_get_chat_messages() {
        let db = test_db();
        let msg = ChatMessage {
            id: "m1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "hello".to_string(),
            topic: "festival/f1".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
        };
        db.save_chat_message(&msg).unwrap();
        let msgs = db.get_chat_messages("festival/f1", 10, 0).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "m1");
        assert_eq!(msgs[0].text, "hello");
    }

    #[test]
    fn test_chat_messages_filtered_by_topic() {
        let db = test_db();
        for i in 0..3 {
            db.save_chat_message(&ChatMessage {
                id: format!("m{i}"),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: format!("msg {i}"),
                topic: "topic/a".to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T2{i}:00:00Z"),
            })
            .unwrap();
        }
        db.save_chat_message(&ChatMessage {
            id: "mx".to_string(),
            user_id: "u2".to_string(),
            display_name: "Bob".to_string(),
            text: "other".to_string(),
            topic: "topic/b".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
        })
        .unwrap();
        let msgs = db.get_chat_messages("topic/a", 10, 0).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn test_save_and_get_gossip() {
        let db = test_db();
        let seq1 = db.save_gossip("topic/a", b"data1").unwrap();
        let seq2 = db.save_gossip("topic/a", b"data2").unwrap();
        assert!(seq2 > seq1);

        let entries = db.get_gossip_since("topic/a", 0).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].1, b"data1");
        assert_eq!(entries[1].1, b"data2");
    }

    #[test]
    fn test_gossip_since_seq() {
        let db = test_db();
        let seq1 = db.save_gossip("topic/a", b"data1").unwrap();
        db.save_gossip("topic/a", b"data2").unwrap();

        let entries = db.get_gossip_since("topic/a", seq1).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, b"data2");
    }

    #[test]
    fn test_gossip_filtered_by_topic() {
        let db = test_db();
        db.save_gossip("topic/a", b"data_a").unwrap();
        db.save_gossip("topic/b", b"data_b").unwrap();

        let entries = db.get_gossip_since("topic/a", 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, b"data_a");
    }
}
