use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine as _;
use yrs::any::Any;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, Map, Out, ReadTxn, StateVector, Transact, Update};

use crate::crypto;
use crate::db::Database;
use crate::signing;
use crate::types::SignedUpdate;

pub struct DocManager {
    docs: HashMap<String, Doc>,
    db: Arc<Database>,
}

impl DocManager {
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            docs: HashMap::new(),
            db,
        }
    }

    /// Get an existing doc or create a new one, loading from DB if available.
    pub fn get_or_create(&mut self, doc_id: &str) -> &Doc {
        if !self.docs.contains_key(doc_id) {
            let doc = Doc::new();
            if let Ok(Some(data)) = self.db.load_doc(doc_id)
                && let Ok(update) = Update::decode_v1(&data)
            {
                let mut txn = doc.transact_mut();
                let _ = txn.apply_update(update);
            }
            self.docs.insert(doc_id.to_string(), doc);
        }
        &self.docs[doc_id]
    }

    /// Apply a raw Yrs update to a doc and persist the new state.
    pub fn apply_update(&mut self, doc_id: &str, update_bytes: &[u8]) -> anyhow::Result<()> {
        let doc = self.get_or_create(doc_id);
        let update = Update::decode_v1(update_bytes)?;
        {
            let mut txn = doc.transact_mut();
            txn.apply_update(update)?;
        }
        self.persist(doc_id)
    }

    /// Encode the state vector of the given doc.
    pub fn get_state_vector(&self, doc_id: &str) -> anyhow::Result<Vec<u8>> {
        let doc = self
            .docs
            .get(doc_id)
            .ok_or_else(|| anyhow::anyhow!("doc not found: {doc_id}"))?;
        let txn = doc.transact();
        Ok(txn.state_vector().encode_v1())
    }

    /// Encode a diff from the remote's state vector to this doc's current state.
    pub fn encode_diff(&self, doc_id: &str, remote_sv_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
        let doc = self
            .docs
            .get(doc_id)
            .ok_or_else(|| anyhow::anyhow!("doc not found: {doc_id}"))?;
        let remote_sv = StateVector::decode_v1(remote_sv_bytes)?;
        let txn = doc.transact();
        Ok(txn.encode_state_as_update_v1(&remote_sv))
    }

    /// Persist the full doc state to the database.
    pub fn persist(&self, doc_id: &str) -> anyhow::Result<()> {
        let doc = self
            .docs
            .get(doc_id)
            .ok_or_else(|| anyhow::anyhow!("doc not found: {doc_id}"))?;
        let txn = doc.transact();
        let empty_sv = StateVector::default();
        let full_update = txn.encode_state_as_update_v1(&empty_sv);
        self.db.save_doc(doc_id, "yrs", &full_update)?;
        Ok(())
    }

    // --- Festival doc helpers ---

    /// Apply a signed update, verifying the Ed25519 signature first.
    /// Rejects updates with invalid signatures.
    pub fn apply_signed_update(
        &mut self,
        doc_id: &str,
        signed: &SignedUpdate,
        public_key: &[u8; 32],
    ) -> anyhow::Result<()> {
        let engine = base64::engine::general_purpose::STANDARD;
        let update_bytes = engine
            .decode(&signed.update)
            .map_err(|e| anyhow::anyhow!("base64 decode update: {e}"))?;
        let sig_bytes = engine
            .decode(&signed.signature)
            .map_err(|e| anyhow::anyhow!("base64 decode signature: {e}"))?;

        if !signing::verify(public_key, &update_bytes, &sig_bytes) {
            anyhow::bail!("invalid signature on update from author {}", signed.author);
        }

        self.apply_update(doc_id, &update_bytes)
    }

    /// Read a string value from the doc's root map.
    /// Loads the doc from the DB if it hasn't been accessed yet.
    pub fn read_map_value(&mut self, doc_id: &str, key: &str) -> Option<String> {
        self.get_or_create(doc_id);
        let doc = self.docs.get(doc_id)?;
        let map = doc.get_or_insert_map("root");
        let txn = doc.transact();
        match map.get(&txn, key) {
            Some(Out::Any(Any::String(s))) => Some(s.to_string()),
            _ => None,
        }
    }

    /// Read all key-value pairs from the root map of a doc.
    ///
    /// Returns a `Vec<(key, value)>` where value is the string representation.
    /// Non-string entries are silently skipped.
    pub fn read_map_values_with_prefix(&mut self, doc_id: &str) -> Vec<(String, String)> {
        self.get_or_create(doc_id);
        let doc = match self.docs.get(doc_id) {
            Some(d) => d,
            None => return vec![],
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
        &mut self,
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

    /// Set a value in the root map of a doc. Returns the encoded update bytes.
    pub fn set_map_value(
        &mut self,
        doc_id: &str,
        key: &str,
        value: &str,
    ) -> anyhow::Result<Vec<u8>> {
        let doc = self.get_or_create(doc_id);

        // Capture state before mutation so we can diff
        let sv_before = {
            let txn = doc.transact();
            txn.state_vector()
        };

        {
            let map = doc.get_or_insert_map("root");
            let mut txn = doc.transact_mut();
            map.insert(&mut txn, key, value);
        }

        // Encode only the new changes (diff from sv_before)
        let txn = doc.transact();
        let update = txn.encode_state_as_update_v1(&sv_before);
        drop(txn);

        self.persist(doc_id)?;
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
        let mut mgr = DocManager::new(db.clone());

        // Set a value
        mgr.set_map_value("doc1", "greeting", "hello").unwrap();

        // Reload into fresh manager
        let mut mgr2 = DocManager::new(db);
        let val = mgr2.read_map_value("doc1", "greeting");
        assert_eq!(val, Some("hello".to_string()));
    }

    #[test]
    fn test_apply_signed_update_valid() {
        let db = test_db();
        let mut mgr = DocManager::new(db.clone());

        // Create update via set_map_value, then sign it
        let signing_key = signing::generate_signing_key();
        let public_key: [u8; 32] = signing_key.verifying_key().to_bytes();

        // Produce a raw update from a scratch doc
        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        let sv_empty = StateVector::default();
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "event", "main stage");
        }
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&sv_empty);

        let engine = base64::engine::general_purpose::STANDARD;
        let sig_bytes = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: engine.encode(&update_bytes),
            author: "organiser".to_string(),
            signature: engine.encode(&sig_bytes),
        };

        mgr.apply_signed_update("doc1", &signed, &public_key)
            .unwrap();
        let val = mgr.read_map_value("doc1", "event");
        assert_eq!(val, Some("main stage".to_string()));
    }

    #[test]
    fn test_apply_signed_update_invalid_sig_rejected() {
        let db = test_db();
        let mut mgr = DocManager::new(db);

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
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&sv_empty);

        let engine = base64::engine::general_purpose::STANDARD;
        let sig_bytes = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: engine.encode(&update_bytes),
            author: "attacker".to_string(),
            signature: engine.encode(&sig_bytes),
        };

        // Verify with the wrong public key → should fail
        let result = mgr.apply_signed_update("doc1", &signed, &wrong_public);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_decrypt_update_roundtrip() {
        let db = test_db();
        let mut mgr_a = DocManager::new(db.clone());
        let mut mgr_b = DocManager::new(db);

        let group_key = crypto::generate_group_key();

        // mgr_a creates a change
        let update = mgr_a
            .set_map_value("shared-doc", "campsite", "field-B")
            .unwrap();

        // Encrypt the update
        let encrypted = mgr_a.encrypt_update(&update, &group_key).unwrap();

        // mgr_b applies the encrypted update
        mgr_b
            .apply_encrypted_update("shared-doc", &encrypted, &group_key)
            .unwrap();

        let val = mgr_b.read_map_value("shared-doc", "campsite");
        assert_eq!(val, Some("field-B".to_string()));
    }

    #[test]
    fn test_state_vector_and_diff_sync() {
        let db = test_db();
        let mut mgr_a = DocManager::new(db.clone());
        let mut mgr_b = DocManager::new(db);

        // mgr_a makes changes
        mgr_a.set_map_value("sync-doc", "key1", "val1").unwrap();
        mgr_a.set_map_value("sync-doc", "key2", "val2").unwrap();

        // mgr_b gets sv (empty doc)
        mgr_b.get_or_create("sync-doc");
        let sv_b = mgr_b.get_state_vector("sync-doc").unwrap();

        // mgr_a encodes diff
        let diff = mgr_a.encode_diff("sync-doc", &sv_b).unwrap();

        // mgr_b applies diff
        mgr_b.apply_update("sync-doc", &diff).unwrap();

        assert_eq!(
            mgr_b.read_map_value("sync-doc", "key1"),
            Some("val1".to_string())
        );
        assert_eq!(
            mgr_b.read_map_value("sync-doc", "key2"),
            Some("val2".to_string())
        );
    }
}
