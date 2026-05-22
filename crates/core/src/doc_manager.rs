use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use yrs::any::Any;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, Map, Out, ReadTxn, StateVector, Transact, Update};

use crate::crypto;
use crate::db::Database;
use crate::signing;
use crate::types::SignedUpdate;

/// Threshold of individual updates before triggering compaction.
const COMPACTION_THRESHOLD: u32 = 100;

/// Manages Yrs CRDT documents with per-document locking.
///
/// All methods take `&self` — concurrent access to different docs is lock-free.
/// Same-doc access is protected by per-doc `RwLock`.
pub struct DocManager {
    docs: DashMap<String, Arc<RwLock<Doc>>>,
    db: Arc<Database>,
}

impl DocManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            docs: DashMap::new(),
            db,
        }
    }

    /// Get an existing doc or create a new one, loading from DB if available.
    ///
    /// Returns `Arc<RwLock<Doc>>` — caller takes read or write lock as needed.
    pub fn get_or_create(&self, doc_id: &str) -> Arc<RwLock<Doc>> {
        self.docs
            .entry(doc_id.to_string())
            .or_insert_with(|| {
                let doc = Doc::new();

                // Try loading compacted state first
                if let Ok(Some(data)) = self.db.load_doc(doc_id)
                    && let Ok(update) = Update::decode_v1(&data)
                {
                    let mut txn = doc.transact_mut();
                    let _ = txn.apply_update(update);
                }

                // Then replay any incremental updates
                if let Ok(updates) = self.db.load_doc_updates(doc_id) {
                    for update_data in updates {
                        if let Ok(update) = Update::decode_v1(&update_data) {
                            let mut txn = doc.transact_mut();
                            let _ = txn.apply_update(update);
                        }
                    }
                }

                Arc::new(RwLock::new(doc))
            })
            .value()
            .clone()
    }

    /// Apply a raw Yrs update to a doc and append to the update log.
    pub fn apply_update(&self, doc_id: &str, update_bytes: &[u8]) -> anyhow::Result<()> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.write().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;
        let update = Update::decode_v1(update_bytes)?;
        {
            let mut txn = doc.transact_mut();
            txn.apply_update(update)?;
        }
        drop(doc);

        self.db.append_doc_update(doc_id, update_bytes)?;

        if let Ok(count) = self.db.count_doc_updates(doc_id)
            && count >= COMPACTION_THRESHOLD
        {
            let _ = self.compact(doc_id);
        }
        Ok(())
    }

    /// Encode the state vector of the given doc.
    pub fn get_state_vector(&self, doc_id: &str) -> anyhow::Result<Vec<u8>> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.read().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;
        let txn = doc.transact();
        Ok(txn.state_vector().encode_v1())
    }

    /// Encode a diff from the remote's state vector to this doc's current state.
    pub fn encode_diff(&self, doc_id: &str, remote_sv_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.read().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;
        let remote_sv = StateVector::decode_v1(remote_sv_bytes)?;
        let txn = doc.transact();
        Ok(txn.encode_state_as_update_v1(&remote_sv))
    }

    /// Encode the full doc state as a single update (from empty state vector).
    pub fn encode_full_state(&self, doc_id: &str) -> anyhow::Result<Vec<u8>> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.read().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;
        let txn = doc.transact();
        Ok(txn.encode_state_as_update_v1(&StateVector::default()))
    }

    /// Compact all updates for a doc into a single blob.
    pub fn compact(&self, doc_id: &str) -> anyhow::Result<()> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.read().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;
        let txn = doc.transact();
        let full_state = txn.encode_state_as_update_v1(&StateVector::default());
        drop(txn);
        drop(doc);
        self.db.compact_doc_updates(doc_id, &full_state)?;
        tracing::debug!("compacted doc {doc_id}");
        Ok(())
    }

    /// Persist the full doc state to the `docs` table (for fast boot loading).
    pub fn persist(&self, doc_id: &str) -> anyhow::Result<()> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.read().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;
        let txn = doc.transact();
        let full_update = txn.encode_state_as_update_v1(&StateVector::default());
        self.db.save_doc(doc_id, "yrs", &full_update)?;
        Ok(())
    }

    /// Persist the doc only if it exists in memory.
    pub fn persist_if_dirty(&self, doc_id: &str) -> anyhow::Result<()> {
        if self.docs.contains_key(doc_id) {
            self.persist(doc_id)?;
        }
        Ok(())
    }

    // --- Festival doc helpers ---

    /// Apply a signed update, verifying the Ed25519 signature first.
    pub fn apply_signed_update(
        &self,
        doc_id: &str,
        signed: &SignedUpdate,
        public_key: &[u8; 32],
    ) -> anyhow::Result<()> {
        if !signing::verify(public_key, &signed.update, &signed.signature) {
            anyhow::bail!("invalid signature on update from author {}", signed.author);
        }
        self.apply_update(doc_id, &signed.update)
    }

    /// Read a string value from the doc's root map.
    pub fn read_map_value(&self, doc_id: &str, key: &str) -> Option<String> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.read().ok()?;
        let map = doc.get_or_insert_map("root");
        let txn = doc.transact();
        match map.get(&txn, key) {
            Some(Out::Any(Any::String(s))) => Some(s.to_string()),
            _ => None,
        }
    }

    /// Read all key-value pairs from the root map of a doc.
    pub fn read_map_values_with_prefix(&self, doc_id: &str) -> Vec<(String, String)> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = match doc_arc.read() {
            Ok(d) => d,
            Err(_) => return vec![],
        };
        let map = doc.get_or_insert_map("root");
        let txn = doc.transact();
        let mut out = Vec::new();
        for (k, v) in map.iter(&txn) {
            if let Out::Any(Any::String(s)) = v {
                out.push((k.to_string(), s.to_string()));
            }
        }
        out
    }

    // --- Group doc helpers ---

    /// Decrypt and apply an encrypted update.
    pub fn apply_encrypted_update(
        &self,
        doc_id: &str,
        encrypted: &[u8],
        group_key: &[u8; 32],
    ) -> anyhow::Result<()> {
        let update_bytes = crypto::decrypt(group_key, encrypted)?;
        self.apply_update(doc_id, &update_bytes)
    }

    /// Encrypt a raw update with the group key.
    pub fn encrypt_update(
        &self,
        update: &[u8],
        group_key: &[u8; 32],
    ) -> anyhow::Result<Vec<u8>> {
        crypto::encrypt(group_key, update)
    }

    /// Remove a key from the root map of a doc. Returns the encoded update bytes.
    pub fn remove_map_value(
        &self,
        doc_id: &str,
        key: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.write().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;

        let sv_before = {
            let txn = doc.transact();
            txn.state_vector()
        };

        {
            let map = doc.get_or_insert_map("root");
            let mut txn = doc.transact_mut();
            map.remove(&mut txn, key);
        }

        let txn = doc.transact();
        let update = txn.encode_state_as_update_v1(&sv_before);
        drop(txn);
        drop(doc);

        self.db.append_doc_update(doc_id, &update)?;
        Ok(update)
    }

    /// Set a value in the root map of a doc. Returns the encoded update bytes.
    pub fn set_map_value(
        &self,
        doc_id: &str,
        key: &str,
        value: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc.write().map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;

        let sv_before = {
            let txn = doc.transact();
            txn.state_vector()
        };

        {
            let map = doc.get_or_insert_map("root");
            let mut txn = doc.transact_mut();
            map.insert(&mut txn, key, value);
        }

        let txn = doc.transact();
        let update = txn.encode_state_as_update_v1(&sv_before);
        drop(txn);
        drop(doc);

        self.db.append_doc_update(doc_id, &update)?;
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    #[test]
    fn test_persist_and_reload() {
        let db = test_db();
        let mgr = DocManager::new(db.clone());

        mgr.set_map_value("doc1", "greeting", "hello").unwrap();
        mgr.persist("doc1").unwrap();

        let mgr2 = DocManager::new(db);
        let val = mgr2.read_map_value("doc1", "greeting");
        assert_eq!(val, Some("hello".to_string()));
    }

    #[test]
    fn test_append_only_reload() {
        let db = test_db();
        let mgr = DocManager::new(db.clone());

        mgr.set_map_value("doc1", "k1", "v1").unwrap();
        mgr.set_map_value("doc1", "k2", "v2").unwrap();

        let mgr2 = DocManager::new(db);
        assert_eq!(mgr2.read_map_value("doc1", "k1"), Some("v1".to_string()));
        assert_eq!(mgr2.read_map_value("doc1", "k2"), Some("v2".to_string()));
    }

    #[test]
    fn test_compaction() {
        let db = test_db();
        let mgr = DocManager::new(db.clone());

        for i in 0..5 {
            mgr.set_map_value("doc1", &format!("k{i}"), &format!("v{i}")).unwrap();
        }

        assert_eq!(db.count_doc_updates("doc1").unwrap(), 5);

        mgr.compact("doc1").unwrap();
        assert_eq!(db.count_doc_updates("doc1").unwrap(), 1);

        let mgr2 = DocManager::new(db);
        for i in 0..5 {
            assert_eq!(
                mgr2.read_map_value("doc1", &format!("k{i}")),
                Some(format!("v{i}"))
            );
        }
    }

    #[test]
    fn test_apply_signed_update_valid() {
        let db = test_db();
        let mgr = DocManager::new(db.clone());

        let signing_key = signing::generate_signing_key();
        let public_key: [u8; 32] = signing_key.verifying_key().to_bytes();

        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        let sv_empty = StateVector::default();
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "event", "main stage");
        }
        let update_bytes = update_doc.transact().encode_state_as_update_v1(&sv_empty);

        let sig_bytes = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: update_bytes,
            author: "organiser".to_string(),
            signature: sig_bytes,
        };

        mgr.apply_signed_update("doc1", &signed, &public_key).unwrap();
        let val = mgr.read_map_value("doc1", "event");
        assert_eq!(val, Some("main stage".to_string()));
    }

    #[test]
    fn test_apply_signed_update_invalid_sig_rejected() {
        let db = test_db();
        let mgr = DocManager::new(db);

        let signing_key = signing::generate_signing_key();
        let wrong_key = signing::generate_signing_key();
        let wrong_public: [u8; 32] = wrong_key.verifying_key().to_bytes();

        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        let sv_empty = StateVector::default();
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "key", "value");
        }
        let update_bytes = update_doc.transact().encode_state_as_update_v1(&sv_empty);

        let sig_bytes = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: update_bytes,
            author: "attacker".to_string(),
            signature: sig_bytes,
        };

        let result = mgr.apply_signed_update("doc1", &signed, &wrong_public);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_decrypt_update_roundtrip() {
        let db = test_db();
        let mgr_a = DocManager::new(db.clone());
        let mgr_b = DocManager::new(db);

        let group_key = crypto::generate_group_key();

        let update = mgr_a.set_map_value("shared-doc", "campsite", "field-B").unwrap();
        let encrypted = mgr_a.encrypt_update(&update, &group_key).unwrap();

        mgr_b.apply_encrypted_update("shared-doc", &encrypted, &group_key).unwrap();

        let val = mgr_b.read_map_value("shared-doc", "campsite");
        assert_eq!(val, Some("field-B".to_string()));
    }

    #[test]
    fn test_remove_map_value_deletes_key() {
        let db = test_db();
        let mgr = DocManager::new(db);

        mgr.set_map_value("doc1", "temp", "data").unwrap();
        assert_eq!(mgr.read_map_value("doc1", "temp"), Some("data".to_string()));

        mgr.remove_map_value("doc1", "temp").unwrap();
        assert_eq!(mgr.read_map_value("doc1", "temp"), None);
    }

    #[test]
    fn test_state_vector_and_diff_sync() {
        let db = test_db();
        let mgr_a = DocManager::new(db.clone());
        let mgr_b = DocManager::new(db);

        mgr_a.set_map_value("sync-doc", "key1", "val1").unwrap();
        mgr_a.set_map_value("sync-doc", "key2", "val2").unwrap();

        mgr_b.get_or_create("sync-doc");
        let sv_b = mgr_b.get_state_vector("sync-doc").unwrap();

        let diff = mgr_a.encode_diff("sync-doc", &sv_b).unwrap();
        mgr_b.apply_update("sync-doc", &diff).unwrap();

        assert_eq!(mgr_b.read_map_value("sync-doc", "key1"), Some("val1".to_string()));
        assert_eq!(mgr_b.read_map_value("sync-doc", "key2"), Some("val2".to_string()));
    }

    #[test]
    fn test_lineup_crdt_roundtrip() {
        let stages = r##"[{"id":"s1","name":"Main Stage","short":"MS","color":"#ff0000","order":0}]"##;
        let days = r#"[{"id":"d1","label":"Friday","num":13,"month":"Jun"}]"#;
        let sets = r#"[{"id":"set1","day":"d1","stage":"s1","artist":"Test Act","startMin":720,"durationMin":60,"genre":"rock","cancelled":false}]"#;

        let server_doc = Doc::new();
        let root = server_doc.get_or_insert_map("root");
        {
            let mut txn = server_doc.transact_mut();
            root.insert(&mut txn, "stages", stages);
            root.insert(&mut txn, "days", days);
            root.insert(&mut txn, "sets", sets);
        }
        let update_bytes = server_doc.transact().encode_state_as_update_v1(&StateVector::default());

        let db = test_db();
        let mgr = DocManager::new(db);
        mgr.apply_update("festival/test/state", &update_bytes).unwrap();

        assert_eq!(mgr.read_map_value("festival/test/state", "stages"), Some(stages.to_string()));
        assert_eq!(mgr.read_map_value("festival/test/state", "days"), Some(days.to_string()));
        assert_eq!(mgr.read_map_value("festival/test/state", "sets"), Some(sets.to_string()));
    }

    #[test]
    fn test_read_lineup_from_empty_doc_returns_none() {
        let db = test_db();
        let mgr = DocManager::new(db);
        assert_eq!(mgr.read_map_value("festival/missing/state", "stages"), None);
    }

    #[test]
    fn test_concurrent_different_docs() {
        use std::thread;

        let db = test_db();
        let mgr = Arc::new(DocManager::new(db));

        let handles: Vec<_> = (0..4)
            .map(|i| {
                let mgr = mgr.clone();
                thread::spawn(move || {
                    let doc_id = format!("doc-{i}");
                    mgr.set_map_value(&doc_id, "key", &format!("val-{i}")).unwrap();
                    let val = mgr.read_map_value(&doc_id, "key");
                    assert_eq!(val, Some(format!("val-{i}")));
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
    }
}
