mod migrations;

use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;

use crate::types::ChatMessage;

/// Thread-safe SQLite database wrapper.
pub struct Database {
    conn: Mutex<Connection>,
}

// SAFETY: `Connection` is `Send` (rusqlite documents this), and we guard
// all access with a `Mutex`, so `Database` can be shared across threads.
unsafe impl Sync for Database {}

impl Database {
    /// Open (or create) a database at the given path and run migrations.
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Self::apply_pragmas(&conn)?;
        migrations::apply_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Create an in-memory database (for tests).
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::apply_pragmas(&conn)?;
        migrations::apply_migrations(&conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn apply_pragmas(conn: &Connection) -> Result<()> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "busy_timeout", "5000")?;
        Ok(())
    }

    // --- docs ---

    pub fn save_doc(&self, id: &str, doc_type: &str, data: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO docs (id, doc_type, data, updated_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            params![id, doc_type, data],
        )?;
        Ok(())
    }

    pub fn load_doc(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT data FROM docs WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Returns the IDs of all docs of the given type.
    pub fn list_docs(&self, doc_type: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id FROM docs WHERE doc_type = ?1 ORDER BY updated_at DESC")?;
        let ids = stmt
            .query_map(params![doc_type], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    // --- doc updates (append-only CRDT persistence) ---

    /// Append a single CRDT update for a doc.
    pub fn append_doc_update(&self, doc_id: &str, update_data: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO doc_updates (doc_id, update_data) VALUES (?1, ?2)",
            params![doc_id, update_data],
        )?;
        Ok(())
    }

    /// Load all update blobs for a doc, ordered by insertion.
    pub fn load_doc_updates(&self, doc_id: &str) -> Result<Vec<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT update_data FROM doc_updates WHERE doc_id = ?1 ORDER BY id",
        )?;
        let updates = stmt
            .query_map(params![doc_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<Vec<u8>>>>()?;
        Ok(updates)
    }

    /// Count updates for a doc.
    pub fn count_doc_updates(&self, doc_id: &str) -> Result<u32> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM doc_updates WHERE doc_id = ?1",
            params![doc_id],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    /// Replace all updates for a doc with a single compacted blob.
    pub fn compact_doc_updates(&self, doc_id: &str, compacted: &[u8]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM doc_updates WHERE doc_id = ?1",
            params![doc_id],
        )?;
        conn.execute(
            "INSERT INTO doc_updates (doc_id, update_data) VALUES (?1, ?2)",
            params![doc_id, compacted],
        )?;
        // Also update the docs table for fast boot
        conn.execute(
            "INSERT OR REPLACE INTO docs (id, doc_type, data, updated_at)
             VALUES (?1, 'yrs', ?2, datetime('now'))",
            params![doc_id, compacted],
        )?;
        Ok(())
    }

    // --- groups ---

    pub fn save_group(&self, id: &str, festival_id: &str, name: &str, key: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO groups (id, festival_id, name, key, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            params![id, festival_id, name, key],
        )?;
        Ok(())
    }

    /// Returns (id, name, key) tuples for all groups of the given festival.
    pub fn load_groups(&self, festival_id: &str) -> Result<Vec<(String, String, Vec<u8>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, name, key FROM groups WHERE festival_id = ?1")?;
        let groups = stmt
            .query_map(params![festival_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(groups)
    }

    pub fn delete_group(&self, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute("DELETE FROM groups WHERE id = ?1", params![id])?;
        Ok(())
    }

    // --- starred sets ---

    /// Toggle a star on a set. Returns the new starred state (true = now starred).
    pub fn toggle_star(&self, festival_id: &str, set_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM starred_sets WHERE festival_id = ?1 AND set_id = ?2",
            params![festival_id, set_id],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if exists {
            conn.execute(
                "DELETE FROM starred_sets WHERE festival_id = ?1 AND set_id = ?2",
                params![festival_id, set_id],
            )?;
            Ok(false)
        } else {
            conn.execute(
                "INSERT INTO starred_sets (festival_id, set_id, starred_at)
                 VALUES (?1, ?2, datetime('now'))",
                params![festival_id, set_id],
            )?;
            Ok(true)
        }
    }

    /// Returns the set IDs that are starred for a festival.
    pub fn get_stars(&self, festival_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT set_id FROM starred_sets WHERE festival_id = ?1")?;
        let ids = stmt
            .query_map(params![festival_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;
        Ok(ids)
    }

    // --- chat messages ---

    pub fn save_chat_message(&self, msg: &ChatMessage) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR IGNORE INTO chat_messages
             (id, topic, user_id, display_name, text, stage_id, timestamp, writer_seq, received_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
            params![
                msg.id,
                msg.topic,
                msg.user_id,
                msg.display_name,
                msg.text,
                msg.stage_id,
                msg.timestamp,
                msg.writer_seq as i64,
            ],
        )?;
        Ok(())
    }

    pub fn save_chat_messages_batch(&self, msgs: &[ChatMessage]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO chat_messages
                 (id, topic, user_id, display_name, text, stage_id, timestamp, writer_seq, received_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, datetime('now'))",
            )?;
            for msg in msgs {
                stmt.execute(params![
                    msg.id,
                    msg.topic,
                    msg.user_id,
                    msg.display_name,
                    msg.text,
                    msg.stage_id,
                    msg.timestamp,
                    msg.writer_seq as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_chat_messages(
        &self,
        topic: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<ChatMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, display_name, text, topic, stage_id, timestamp, writer_seq
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
                    writer_seq: row.get::<_, i64>(7)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(msgs)
    }

    /// Get the next writer_seq for a user on a topic (max + 1, or 1 if none).
    pub fn get_next_writer_seq(&self, topic: &str, user_id: &str) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let max: i64 = conn.query_row(
            "SELECT COALESCE(MAX(writer_seq), 0) FROM chat_messages WHERE topic = ?1 AND user_id = ?2",
            params![topic, user_id],
            |row| row.get(0),
        )?;
        Ok((max + 1) as u64)
    }

    /// Compute the chat state vector for a topic: {user_id: max_writer_seq} for each writer.
    pub fn compute_chat_sv(&self, topic: &str) -> Result<std::collections::HashMap<String, u64>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT user_id, MAX(writer_seq) FROM chat_messages WHERE topic = ?1 GROUP BY user_id",
        )?;
        let sv = stmt
            .query_map(params![topic], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<rusqlite::Result<std::collections::HashMap<String, u64>>>()?;
        Ok(sv)
    }

    /// Get messages that are newer than the given state vector.
    /// Returns messages where user's writer_seq > sv[user] OR user not in sv.
    pub fn get_messages_since_sv(
        &self,
        topic: &str,
        sv: &std::collections::HashMap<String, u64>,
        limit: u32,
    ) -> Result<Vec<ChatMessage>> {
        // Simple approach: get all messages for topic, filter in Rust
        // (More efficient SQL is possible but this is clearer and the message count is bounded)
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, display_name, text, topic, stage_id, timestamp, writer_seq
             FROM chat_messages
             WHERE topic = ?1
             ORDER BY writer_seq DESC, timestamp DESC
             LIMIT ?2",
        )?;
        let all_msgs: Vec<ChatMessage> = stmt
            .query_map(params![topic, limit * 10], |row| {
                // over-fetch to filter
                Ok(ChatMessage {
                    id: row.get(0)?,
                    user_id: row.get(1)?,
                    display_name: row.get(2)?,
                    text: row.get(3)?,
                    topic: row.get(4)?,
                    stage_id: row.get(5)?,
                    timestamp: row.get(6)?,
                    writer_seq: row.get::<_, i64>(7)? as u64,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let filtered: Vec<ChatMessage> = all_msgs
            .into_iter()
            .filter(|m| match sv.get(&m.user_id) {
                Some(&max_seq) => m.writer_seq > max_seq,
                None => true, // unknown writer → include all
            })
            .take(limit as usize)
            .collect();

        Ok(filtered)
    }

    // --- credentials ---

    /// Read a named credential value (arbitrary bytes).
    pub fn get_credential(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM credentials WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// Upsert a named credential value.
    pub fn set_credential(&self, key: &str, value: &[u8]) -> Result<()> {
        self.conn.lock().unwrap().execute(
            "INSERT OR REPLACE INTO credentials (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    // --- iroh secret key ---

    /// Credential key used to persist the iroh node secret key.
    const IROH_SECRET_KEY: &str = "iroh_secret_key";

    /// Load a previously persisted iroh `SecretKey`, if any.
    pub fn load_iroh_secret_key(&self) -> Result<Option<iroh::SecretKey>> {
        let blob = self.get_credential(Self::IROH_SECRET_KEY)?;
        match blob {
            Some(bytes) => {
                let arr: [u8; 32] = bytes
                    .try_into()
                    .map_err(|v: Vec<u8>| anyhow::anyhow!(
                        "iroh secret key has wrong length: expected 32 bytes, got {}",
                        v.len()
                    ))?;
                Ok(Some(iroh::SecretKey::from_bytes(&arr)))
            }
            None => Ok(None),
        }
    }

    /// Persist an iroh `SecretKey` so it survives restarts.
    pub fn save_iroh_secret_key(&self, key: &iroh::SecretKey) -> Result<()> {
        self.set_credential(Self::IROH_SECRET_KEY, &key.to_bytes())
    }

    // --- group key lookup ---

    /// Load the AES key for a group, returning None if not found.
    pub fn load_group_key(&self, group_id: &str) -> Result<Option<[u8; 32]>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT key FROM groups WHERE id = ?1")?;
        let mut rows = stmt.query(params![group_id])?;
        if let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| anyhow::anyhow!("group key has wrong length"))?;
            Ok(Some(arr))
        } else {
            Ok(None)
        }
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
            writer_seq: 0,
        };
        db.save_chat_message(&msg).unwrap();
        let msgs = db.get_chat_messages("festival/f1", 10, 0).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].id, "m1");
        assert_eq!(msgs[0].text, "hello");
    }

    #[test]
    fn test_save_chat_messages_batch() {
        let db = test_db();
        let msgs: Vec<ChatMessage> = (0..100)
            .map(|i| ChatMessage {
                id: format!("m{i}"),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: format!("msg {i}"),
                topic: "topic/batch".to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T20:{:02}:00Z", i % 60),
                writer_seq: i as u64,
            })
            .collect();
        db.save_chat_messages_batch(&msgs).unwrap();
        let loaded = db.get_chat_messages("topic/batch", 200, 0).unwrap();
        assert_eq!(loaded.len(), 100);
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
                writer_seq: i as u64,
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
            writer_seq: 0,
        })
        .unwrap();
        let msgs = db.get_chat_messages("topic/a", 10, 0).unwrap();
        assert_eq!(msgs.len(), 3);
    }

    #[test]
    fn test_chat_message_insert_or_ignore_preserves_received_at() {
        let db = test_db();
        let msg = ChatMessage {
            id: "dedup1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "first".to_string(),
            topic: "topic/a".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 0,
        };
        db.save_chat_message(&msg).unwrap();

        // Read original received_at
        let conn = db.conn.lock().unwrap();
        let original_received_at: String = conn
            .query_row(
                "SELECT received_at FROM chat_messages WHERE id = ?1",
                params!["dedup1"],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);

        // Insert same ID again with different text — should be ignored
        let msg2 = ChatMessage {
            id: "dedup1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "second".to_string(),
            topic: "topic/a".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T21:00:00Z".to_string(),
            writer_seq: 1,
        };
        db.save_chat_message(&msg2).unwrap();

        // received_at and text should be unchanged (original preserved)
        let conn = db.conn.lock().unwrap();
        let (stored_text, stored_received_at): (String, String) = conn
            .query_row(
                "SELECT text, received_at FROM chat_messages WHERE id = ?1",
                params!["dedup1"],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored_text, "first", "INSERT OR IGNORE should preserve original text");
        assert_eq!(
            stored_received_at, original_received_at,
            "INSERT OR IGNORE should preserve original received_at"
        );
    }

    #[test]
    fn test_chat_message_batch_insert_or_ignore() {
        let db = test_db();
        let msg = ChatMessage {
            id: "batch_dedup".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "original".to_string(),
            topic: "topic/b".to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 0,
        };
        db.save_chat_message(&msg).unwrap();

        // Batch insert includes the same ID
        let msgs = vec![
            ChatMessage {
                id: "batch_dedup".to_string(),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: "replaced".to_string(),
                topic: "topic/b".to_string(),
                stage_id: None,
                timestamp: "2026-06-13T21:00:00Z".to_string(),
                writer_seq: 1,
            },
            ChatMessage {
                id: "batch_new".to_string(),
                user_id: "u1".to_string(),
                display_name: "Alice".to_string(),
                text: "new msg".to_string(),
                topic: "topic/b".to_string(),
                stage_id: None,
                timestamp: "2026-06-13T22:00:00Z".to_string(),
                writer_seq: 2,
            },
        ];
        db.save_chat_messages_batch(&msgs).unwrap();

        let stored = db.get_chat_messages("topic/b", 10, 0).unwrap();
        assert_eq!(stored.len(), 2);
        let original = stored.iter().find(|m| m.id == "batch_dedup").unwrap();
        assert_eq!(original.text, "original", "batch INSERT OR IGNORE should preserve original");
    }

    #[test]
    fn test_get_next_writer_seq() {
        let db = test_db();
        let topic = "festival/f1/chat/general";
        let user_id = "u1";

        // No messages yet → seq should be 1
        let seq = db.get_next_writer_seq(topic, user_id).unwrap();
        assert_eq!(seq, 1);

        // Insert a message with writer_seq=1
        db.save_chat_message(&ChatMessage {
            id: "m1".to_string(),
            user_id: user_id.to_string(),
            display_name: "Alice".to_string(),
            text: "hello".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 1,
        })
        .unwrap();

        // Next should be 2
        let seq2 = db.get_next_writer_seq(topic, user_id).unwrap();
        assert_eq!(seq2, 2);
    }

    #[test]
    fn test_compute_chat_sv() {
        let db = test_db();
        let topic = "festival/f1/chat/general";

        db.save_chat_message(&ChatMessage {
            id: "m1".to_string(),
            user_id: "alice".to_string(),
            display_name: "Alice".to_string(),
            text: "hi".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:00:00Z".to_string(),
            writer_seq: 3,
        })
        .unwrap();
        db.save_chat_message(&ChatMessage {
            id: "m2".to_string(),
            user_id: "bob".to_string(),
            display_name: "Bob".to_string(),
            text: "hey".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:01:00Z".to_string(),
            writer_seq: 7,
        })
        .unwrap();
        db.save_chat_message(&ChatMessage {
            id: "m3".to_string(),
            user_id: "alice".to_string(),
            display_name: "Alice".to_string(),
            text: "world".to_string(),
            topic: topic.to_string(),
            stage_id: None,
            timestamp: "2026-06-13T20:02:00Z".to_string(),
            writer_seq: 5,
        })
        .unwrap();

        let sv = db.compute_chat_sv(topic).unwrap();
        assert_eq!(sv.get("alice").copied(), Some(5));
        assert_eq!(sv.get("bob").copied(), Some(7));
    }

    #[test]
    fn test_get_messages_since_sv() {
        let db = test_db();
        let topic = "festival/f1/chat/general";

        // Alice has seqs 1, 2, 3; Bob has seqs 1, 2
        for seq in 1u64..=3 {
            db.save_chat_message(&ChatMessage {
                id: format!("alice-{seq}"),
                user_id: "alice".to_string(),
                display_name: "Alice".to_string(),
                text: format!("alice msg {seq}"),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T20:0{seq}:00Z"),
                writer_seq: seq,
            })
            .unwrap();
        }
        for seq in 1u64..=2 {
            db.save_chat_message(&ChatMessage {
                id: format!("bob-{seq}"),
                user_id: "bob".to_string(),
                display_name: "Bob".to_string(),
                text: format!("bob msg {seq}"),
                topic: topic.to_string(),
                stage_id: None,
                timestamp: format!("2026-06-13T21:0{seq}:00Z"),
                writer_seq: seq,
            })
            .unwrap();
        }

        // sv: alice=2, bob=1 → should return alice-3 and bob-2
        let sv = std::collections::HashMap::from([
            ("alice".to_string(), 2u64),
            ("bob".to_string(), 1u64),
        ]);
        let msgs = db.get_messages_since_sv(topic, &sv, 50).unwrap();
        let ids: Vec<&str> = msgs.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"alice-3"), "expected alice-3 in {ids:?}");
        assert!(ids.contains(&"bob-2"), "expected bob-2 in {ids:?}");
        assert!(!ids.contains(&"alice-1"), "alice-1 should be filtered");
        assert!(!ids.contains(&"alice-2"), "alice-2 should be filtered");
        assert!(!ids.contains(&"bob-1"), "bob-1 should be filtered");

        // sv empty → all messages returned
        let all = db
            .get_messages_since_sv(topic, &std::collections::HashMap::new(), 50)
            .unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn test_wal_mode_enabled() {
        let db = test_db();
        let conn = db.conn.lock().unwrap();
        let mode: String =
            conn.pragma_query_value(None, "journal_mode", |row| row.get(0)).unwrap();
        // In-memory databases use "memory" journal mode, but the pragmas should
        // still be set without error. For on-disk databases this would be "wal".
        // The key assertion is that we can open the DB and the pragma calls succeed.
        assert!(
            mode == "memory" || mode == "wal",
            "expected 'memory' or 'wal', got '{mode}'"
        );
    }

    #[test]
    fn test_wal_mode_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = Database::new(&path).unwrap();
        let conn = db.conn.lock().unwrap();
        let mode: String =
            conn.pragma_query_value(None, "journal_mode", |row| row.get(0)).unwrap();
        assert_eq!(mode, "wal", "on-disk database should use WAL journal mode");
    }

    #[test]
    fn test_busy_timeout_set() {
        let db = test_db();
        let conn = db.conn.lock().unwrap();
        let timeout: i64 =
            conn.pragma_query_value(None, "busy_timeout", |row| row.get(0)).unwrap();
        assert!(timeout >= 5000, "busy_timeout should be at least 5000ms, got {timeout}");
    }

    #[test]
    fn test_iroh_secret_key_roundtrip_in_memory() {
        let db = test_db();

        // Initially no key stored.
        assert!(db.load_iroh_secret_key().unwrap().is_none());

        // Save a key and reload it.
        let key = iroh::SecretKey::generate();
        db.save_iroh_secret_key(&key).unwrap();

        let loaded = db.load_iroh_secret_key().unwrap().expect("key should exist");
        assert_eq!(
            key.public(),
            loaded.public(),
            "loaded key must produce the same public key"
        );
    }

    #[test]
    fn test_secret_key_persistence() {
        // Verify that closing and re-opening the same on-disk database
        // produces the same iroh EndpointId (i.e. the secret key survives).
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("persist.db");

        // First open: generate & store a key.
        let public_key_1 = {
            let db = Database::new(&path).unwrap();
            assert!(db.load_iroh_secret_key().unwrap().is_none());
            let key = iroh::SecretKey::generate();
            db.save_iroh_secret_key(&key).unwrap();
            key.public()
        }; // db dropped, connection closed

        // Second open: key should be loaded from the database.
        let public_key_2 = {
            let db = Database::new(&path).unwrap();
            let loaded = db
                .load_iroh_secret_key()
                .unwrap()
                .expect("key should survive across reopens");
            loaded.public()
        };

        assert_eq!(
            public_key_1, public_key_2,
            "EndpointId must be identical after database reopen"
        );
    }
}
