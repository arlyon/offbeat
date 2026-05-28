use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use dashmap::DashMap;
use yrs::any::Any;
use yrs::types::map::MapRef;
use yrs::updates::decoder::Decode;
use yrs::updates::encoder::Encode;
use yrs::{Doc, Map, Out, ReadTxn, StateVector, Transact, Update};

use crate::crypto;
use crate::db::Database;
use crate::signing;
use crate::types::SignedUpdate;

/// Threshold of individual updates before triggering compaction.
const COMPACTION_THRESHOLD: u32 = 100;

// ---------------------------------------------------------------------------
// Transaction wrappers
// ---------------------------------------------------------------------------

/// Manages Yrs CRDT documents with per-document locking.
///
/// All methods take `&self` — concurrent access to different docs is lock-free.
/// Same-doc access is protected by per-doc `RwLock`.
///
/// # Document layout convention
///
/// Each Yrs `Doc` uses **top-level shared types** (via `doc.get_or_insert_map()`)
/// for every collection that multiple peers may concurrently insert into:
///
/// ```text
///   doc
///   ├── "root"     (YMap)  — scalar metadata (name, festival_id, …)
///   ├── "members"  (YMap)  — keyed by user_id → YMap of fields
///   ├── "pins"     (YMap)  — keyed by pin_id → YMap of fields
///   ├── "stars"    (YMap)  — keyed by user_id → YMap { set_id: true }
///   ├── "stages"   (YMap)  — keyed by stage_id → YMap of fields   (festival docs)
///   ├── "days"     (YMap)  — keyed by day_id → YMap of fields     (festival docs)
///   ├── "sets"     (YMap)  — keyed by set_id → YMap of fields     (festival docs)
///   └── "weather"  (YMap)  — weather metadata + nested "hourly"   (festival docs)
/// ```
///
/// **Why top-level shared types?** When two peers independently call
/// `doc.get_or_insert_map("members")`, Yrs resolves both to the *same* CRDT
/// instance — entries inserted on either peer merge correctly. By contrast, if
/// we used a nested YMap inside root (`root.insert(txn, "members", MapPrelim)`)
/// on two peers independently, Yrs would create *two competing values* for the
/// `"members"` key, and only one would survive the merge (last-writer-wins),
/// silently dropping the other's entries.
///
/// Individual entries *within* these maps (e.g., a specific member record)
/// are created via [`get_or_init_map`] which inserts a `MapPrelim`. This is
/// safe because each entry has a unique key (user_id, pin_id, set_id) and is
/// only created by one peer.
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

    // --- Transaction helpers ---

    /// Execute a mutation on a doc's named maps.
    ///
    /// `map_names` lists the top-level shared types the closure needs (e.g.
    /// `&["root", "members"]`). These are resolved via
    /// `doc.get_or_insert_map()` **before** opening the `TransactionMut`, since
    /// `get_or_insert_map` may internally open an implicit transaction and would
    /// deadlock if a `TransactionMut` is already held.
    ///
    /// The closure receives the resolved maps (same order as `map_names`) and a
    /// mutable transaction.
    ///
    /// Handles write lock, SV capture, update encoding, and persistence.
    /// Returns the encoded update bytes (diff from before the mutation).
    pub fn mutate<F>(&self, doc_id: &str, map_names: &[&str], f: F) -> anyhow::Result<Vec<u8>>
    where
        F: FnOnce(&[MapRef], &mut yrs::TransactionMut),
    {
        let doc_arc = self.get_or_create(doc_id);
        let doc = doc_arc
            .write()
            .map_err(|_| anyhow::anyhow!("doc lock poisoned"))?;

        let sv_before = {
            let txn = doc.transact();
            txn.state_vector()
        };

        // Resolve all named maps before opening the transaction.
        let maps: Vec<MapRef> = map_names
            .iter()
            .map(|name| doc.get_or_insert_map(*name))
            .collect();

        {
            let mut txn = doc.transact_mut();
            f(&maps, &mut txn);
        }

        let txn = doc.transact();
        let update = txn.encode_state_as_update_v1(&sv_before);
        drop(txn);
        drop(doc);

        self.db.append_doc_update(doc_id, &update)?;
        Ok(update)
    }

    /// Execute a read on a doc's named maps.
    ///
    /// `map_names` lists the top-level shared types the closure needs.
    /// Resolved before opening the read transaction (same reason as `mutate`).
    pub fn read<F, T>(&self, doc_id: &str, map_names: &[&str], f: F) -> T
    where
        F: FnOnce(&[MapRef], &yrs::Transaction) -> T,
    {
        let doc_arc = self.get_or_create(doc_id);
        let doc = match doc_arc.read() {
            Ok(d) => d,
            Err(_) => {
                let fresh = Doc::new();
                let maps: Vec<MapRef> = map_names
                    .iter()
                    .map(|name| fresh.get_or_insert_map(*name))
                    .collect();
                let txn = fresh.transact();
                return f(&maps, &txn);
            }
        };
        let maps: Vec<MapRef> = map_names
            .iter()
            .map(|name| doc.get_or_insert_map(*name))
            .collect();
        let txn = doc.transact();
        f(&maps, &txn)
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

    /// Read all entries from a top-level named map in the doc, where each entry
    /// value is itself a YMap. Returns `(key, fields)` pairs.
    pub fn read_nested_map_entries(
        &self,
        doc_id: &str,
        map_name: &str,
    ) -> Vec<(String, HashMap<String, Any>)> {
        self.read(doc_id, &[map_name], |maps, txn| {
            read_map_entries(&maps[0], txn)
        })
    }

    /// Read a single entry from a top-level named map in the doc.
    pub fn read_nested_map_entry(
        &self,
        doc_id: &str,
        map_name: &str,
        entry_key: &str,
    ) -> Option<HashMap<String, Any>> {
        self.read(doc_id, &[map_name], |maps, txn| {
            match maps[0].get(txn, entry_key) {
                Some(Out::YMap(inner)) => read_map_entry_fields(&inner, txn),
                _ => None,
            }
        })
    }

    // --- Peer list helpers ---

    /// Parse the peer list from a festival's CRDT document.
    ///
    /// Reads the `"peers"` YMap from the document's root map, deserialises each
    /// JSON entry, filters out `own_endpoint_id`, and skips malformed entries
    /// with a warning log.
    pub fn parse_peer_list(
        &self,
        festival_id: &str,
        own_endpoint_id: &str,
    ) -> Vec<crate::types::PeerInfo> {
        let doc_arc = self.get_or_create(festival_id);
        let doc = match doc_arc.read() {
            Ok(d) => d,
            Err(_) => return vec![],
        };
        let root = doc.get_or_insert_map("root");
        let txn = doc.transact();

        // The server stores "peers" as a nested YMap.
        let peers_map = match root.get(&txn, "peers") {
            Some(Out::YMap(map_ref)) => map_ref,
            _ => return vec![],
        };

        let mut result = Vec::new();
        for (key, value) in peers_map.iter(&txn) {
            let endpoint_id = key.to_string();

            // Skip our own endpoint
            if endpoint_id == own_endpoint_id {
                continue;
            }

            let json_str = match value {
                Out::Any(Any::String(s)) => s.to_string(),
                _ => {
                    tracing::warn!(
                        endpoint_id = %endpoint_id,
                        "skipping peer entry: value is not a string"
                    );
                    continue;
                }
            };

            #[derive(serde::Deserialize)]
            struct RawPeerEntry {
                relay_url: Option<String>,
                last_seen: u64,
                user_id: String,
            }

            match serde_json::from_str::<RawPeerEntry>(&json_str) {
                Ok(entry) => {
                    result.push(crate::types::PeerInfo {
                        endpoint_id,
                        relay_url: entry.relay_url,
                        last_seen: entry.last_seen,
                        user_id: entry.user_id,
                    });
                }
                Err(e) => {
                    tracing::warn!(
                        endpoint_id = %endpoint_id,
                        error = %e,
                        "skipping peer entry: failed to parse JSON"
                    );
                }
            }
        }

        result
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

    /// Remove a key from the `"root"` top-level map of a doc.
    /// Returns the encoded update bytes.
    pub fn remove_map_value(&self, doc_id: &str, key: &str) -> anyhow::Result<Vec<u8>> {
        let key = key.to_string();
        self.mutate(doc_id, &["root"], |maps, txn| {
            maps[0].remove(txn, &key);
        })
    }

    /// Set a string value in the `"root"` top-level map of a doc.
    /// Returns the encoded update bytes.
    pub fn set_map_value(&self, doc_id: &str, key: &str, value: &str) -> anyhow::Result<Vec<u8>> {
        let key = key.to_string();
        let value = value.to_string();
        self.mutate(doc_id, &["root"], |maps, txn| {
            maps[0].insert(txn, key.as_str(), value);
        })
    }
}

// ---------------------------------------------------------------------------
// Free functions for nested YMap operations
// ---------------------------------------------------------------------------

/// Get or create a nested YMap at `map[key]`.
///
/// Use this for **leaf entries** within a top-level shared map — e.g., a single
/// member record inside the `"members"` map. Because each entry key is unique
/// (user_id, pin_id, etc.) and only one peer creates it, the `MapPrelim`
/// insertion won't conflict.
///
/// **Do NOT use this for collection-level maps** that multiple peers might
/// independently create (like `"members"` itself). For those, use
/// `doc.get_or_insert_map("members")` which is a top-level shared type
/// guaranteed to merge correctly.
pub fn get_or_init_map(map: &MapRef, txn: &mut yrs::TransactionMut, key: &str) -> MapRef {
    match map.get(txn, key) {
        Some(Out::YMap(m)) => m,
        _ => {
            let empty: [(&str, &str); 0] = [];
            map.insert(txn, key, yrs::MapPrelim::from(empty));
            match map.get(txn, key) {
                Some(Out::YMap(m)) => m,
                _ => unreachable!("just inserted a map"),
            }
        }
    }
}

/// Read all entries from a nested YMap within `parent[map_key]`.
/// Each entry's value is expected to be a YMap; its fields are collected into a HashMap.
pub fn read_map_entries_from(
    parent: &MapRef,
    txn: &yrs::Transaction,
    map_key: &str,
) -> Vec<(String, HashMap<String, Any>)> {
    let nested = match parent.get(txn, map_key) {
        Some(Out::YMap(m)) => m,
        _ => return vec![],
    };
    read_map_entries(&nested, txn)
}

/// Read all entries from a YMap where values are YMaps.
pub fn read_map_entries(
    map: &MapRef,
    txn: &yrs::Transaction,
) -> Vec<(String, HashMap<String, Any>)> {
    let mut out = Vec::new();
    for (k, v) in map.iter(txn) {
        if let Out::YMap(inner) = v
            && let Some(fields) = read_map_entry_fields(&inner, txn)
        {
            out.push((k.to_string(), fields));
        }
    }
    out
}

/// Read all fields from a YMap entry, collecting Any values into a HashMap.
pub fn read_map_entry_fields(
    map: &MapRef,
    txn: &yrs::Transaction,
) -> Option<HashMap<String, Any>> {
    let mut fields = HashMap::new();
    for (k, v) in map.iter(txn) {
        if let Out::Any(a) = v {
            fields.insert(k.to_string(), a);
        }
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

// ---------------------------------------------------------------------------
// Any extraction helpers
// ---------------------------------------------------------------------------

/// Extract a string from an Any-valued HashMap.
pub fn any_str(fields: &HashMap<String, Any>, key: &str) -> Option<String> {
    match fields.get(key)? {
        Any::String(s) => Some(s.to_string()),
        _ => None,
    }
}

/// Extract an i32 (from BigInt) from an Any-valued HashMap.
pub fn any_i32(fields: &HashMap<String, Any>, key: &str) -> Option<i32> {
    match fields.get(key)? {
        Any::BigInt(n) => Some(*n as i32),
        Any::Number(n) => Some(*n as i32),
        _ => None,
    }
}

/// Extract an f64 from an Any-valued HashMap.
pub fn any_f64(fields: &HashMap<String, Any>, key: &str) -> Option<f64> {
    match fields.get(key)? {
        Any::Number(n) => Some(*n),
        Any::BigInt(n) => Some(*n as f64),
        _ => None,
    }
}

/// Extract a bool from an Any-valued HashMap.
pub fn any_bool(fields: &HashMap<String, Any>, key: &str) -> Option<bool> {
    match fields.get(key)? {
        Any::Bool(b) => Some(*b),
        _ => None,
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
        use super::get_or_init_map;

        // Simulate server writing lineup data using top-level named maps
        let server_doc = Doc::new();
        {
            let stages_map = server_doc.get_or_insert_map("stages");
            let days_map = server_doc.get_or_insert_map("days");
            let sets_map = server_doc.get_or_insert_map("sets");

            let mut txn = server_doc.transact_mut();

            let s1 = get_or_init_map(&stages_map, &mut txn, "s1");
            s1.insert(&mut txn, "name", "Main Stage");
            s1.insert(&mut txn, "short", "MS");
            s1.insert(&mut txn, "color", "#ff0000");
            s1.insert(&mut txn, "order", 0i64);

            let d1 = get_or_init_map(&days_map, &mut txn, "d1");
            d1.insert(&mut txn, "label", "Friday");
            d1.insert(&mut txn, "num", 13i64);
            d1.insert(&mut txn, "month", "Jun");

            let set1 = get_or_init_map(&sets_map, &mut txn, "set1");
            set1.insert(&mut txn, "day", "d1");
            set1.insert(&mut txn, "stage", "s1");
            set1.insert(&mut txn, "artist", "Test Act");
            set1.insert(&mut txn, "startMin", 720i64);
            set1.insert(&mut txn, "durationMin", 60i64);
            set1.insert(&mut txn, "genre", "rock");
            set1.insert(&mut txn, "cancelled", false);
        }
        let update_bytes = server_doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());

        let db = test_db();
        let mgr = DocManager::new(db);
        mgr.apply_update("festival/test/state", &update_bytes)
            .unwrap();

        let stages = mgr.read_nested_map_entries("festival/test/state", "stages");
        assert_eq!(stages.len(), 1);
        assert_eq!(
            super::any_str(&stages[0].1, "name"),
            Some("Main Stage".to_string())
        );

        let days = mgr.read_nested_map_entries("festival/test/state", "days");
        assert_eq!(days.len(), 1);
        assert_eq!(
            super::any_str(&days[0].1, "label"),
            Some("Friday".to_string())
        );

        let sets = mgr.read_nested_map_entries("festival/test/state", "sets");
        assert_eq!(sets.len(), 1);
        assert_eq!(
            super::any_str(&sets[0].1, "artist"),
            Some("Test Act".to_string())
        );
    }

    #[test]
    fn test_read_lineup_from_empty_doc_returns_none() {
        let db = test_db();
        let mgr = DocManager::new(db);
        assert_eq!(mgr.read_map_value("festival/missing/state", "stages"), None);
    }

    #[test]
    fn test_mutate_and_read_helpers() {
        let db = test_db();
        let mgr = DocManager::new(db);

        mgr.mutate("test-doc", &["root"], |maps, txn| {
            maps[0].insert(txn, "name", "hello");
        })
        .unwrap();

        let val = mgr.read("test-doc", &["root"], |maps, txn| {
            match maps[0].get(txn, "name") {
                Some(Out::Any(Any::String(s))) => Some(s.to_string()),
                _ => None,
            }
        });
        assert_eq!(val, Some("hello".to_string()));
    }

    #[test]
    fn test_nested_map_operations() {
        use super::{any_str, get_or_init_map};

        let db = test_db();
        let mgr = DocManager::new(db);

        // Use top-level named map "members" (not nested inside root)
        mgr.mutate("nested-doc", &["members"], |maps, txn| {
            let alice = get_or_init_map(&maps[0], txn, "alice");
            alice.insert(txn, "displayName", "Alice");
            alice.insert(txn, "status", "active");

            let bob = get_or_init_map(&maps[0], txn, "bob");
            bob.insert(txn, "displayName", "Bob");
            bob.insert(txn, "status", "active");
        })
        .unwrap();

        let entries = mgr.read_nested_map_entries("nested-doc", "members");
        assert_eq!(entries.len(), 2);

        let alice = entries.iter().find(|(k, _)| k == "alice").unwrap();
        assert_eq!(any_str(&alice.1, "displayName"), Some("Alice".to_string()));

        let bob = entries.iter().find(|(k, _)| k == "bob").unwrap();
        assert_eq!(any_str(&bob.1, "displayName"), Some("Bob".to_string()));

        // Read single entry
        let alice_entry = mgr.read_nested_map_entry("nested-doc", "members", "alice");
        assert!(alice_entry.is_some());
        assert_eq!(
            any_str(&alice_entry.unwrap(), "status"),
            Some("active".to_string())
        );

        // Read missing entry
        let missing = mgr.read_nested_map_entry("nested-doc", "members", "charlie");
        assert!(missing.is_none());
    }

    #[test]
    fn test_nested_map_upsert() {
        use super::{any_str, get_or_init_map};

        let db = test_db();
        let mgr = DocManager::new(db);

        mgr.mutate("upsert-doc", &["members"], |maps, txn| {
            let alice = get_or_init_map(&maps[0], txn, "alice");
            alice.insert(txn, "displayName", "Alice");
            alice.insert(txn, "status", "idle");
        })
        .unwrap();

        // Upsert: change status
        mgr.mutate("upsert-doc", &["members"], |maps, txn| {
            let alice = get_or_init_map(&maps[0], txn, "alice");
            alice.insert(txn, "status", "active");
            alice.insert(txn, "stageId", "main-stage");
        })
        .unwrap();

        let alice = mgr
            .read_nested_map_entry("upsert-doc", "members", "alice")
            .unwrap();
        assert_eq!(any_str(&alice, "displayName"), Some("Alice".to_string()));
        assert_eq!(any_str(&alice, "status"), Some("active".to_string()));
        assert_eq!(
            any_str(&alice, "stageId"),
            Some("main-stage".to_string())
        );
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

    /// Helper: insert peer entries into a doc's "root" → "peers" nested YMap.
    fn insert_peer_entries(
        mgr: &DocManager,
        doc_id: &str,
        peers: &[(&str, &str)], // (endpoint_id, json_value)
    ) {
        let doc_arc = mgr.get_or_create(doc_id);
        let doc = doc_arc.write().unwrap();
        let root = doc.get_or_insert_map("root");
        let mut txn = doc.transact_mut();

        // Get or create the nested "peers" map.
        let peers_map = match root.get(&txn, "peers") {
            Some(Out::YMap(map_ref)) => map_ref,
            _ => {
                // Insert a new empty map via MapPrelim.
                let empty: [(&str, &str); 0] = [];
                root.insert(
                    &mut txn,
                    "peers",
                    yrs::MapPrelim::from(empty),
                );
                match root.get(&txn, "peers") {
                    Some(Out::YMap(map_ref)) => map_ref,
                    _ => panic!("failed to create peers map"),
                }
            }
        };

        for (eid, json_val) in peers {
            peers_map.insert(&mut txn, *eid, *json_val);
        }
    }

    #[test]
    fn test_peer_list_parse() {
        let db = test_db();
        let mgr = DocManager::new(db);

        let peer_json_a = r#"{"relay_url":"https://relay.example.com","last_seen":1700000000,"user_id":"user-a"}"#;
        let peer_json_b =
            r#"{"relay_url":null,"last_seen":1700001000,"user_id":"user-b"}"#;

        insert_peer_entries(
            &mgr,
            "fest-1",
            &[
                ("aa".repeat(32).leak(), peer_json_a),
                ("bb".repeat(32).leak(), peer_json_b),
            ],
        );

        let peers = mgr.parse_peer_list("fest-1", "not-me");
        assert_eq!(peers.len(), 2);

        let a = peers.iter().find(|p| p.user_id == "user-a").unwrap();
        assert_eq!(a.relay_url.as_deref(), Some("https://relay.example.com"));
        assert_eq!(a.last_seen, 1700000000);

        let b = peers.iter().find(|p| p.user_id == "user-b").unwrap();
        assert!(b.relay_url.is_none());
        assert_eq!(b.last_seen, 1700001000);
    }

    #[test]
    fn test_peer_list_filters_self() {
        let db = test_db();
        let mgr = DocManager::new(db);

        let own_id: &str = &"cc".repeat(32);
        let other_id: &str = &"dd".repeat(32);

        let json = r#"{"relay_url":null,"last_seen":1700000000,"user_id":"someone"}"#;

        insert_peer_entries(&mgr, "fest-2", &[(own_id, json), (other_id, json)]);

        let peers = mgr.parse_peer_list("fest-2", own_id);
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].endpoint_id, other_id);
    }

    #[test]
    fn test_peer_list_handles_malformed() {
        let db = test_db();
        let mgr = DocManager::new(db);

        let good_json =
            r#"{"relay_url":null,"last_seen":1700000000,"user_id":"good-user"}"#;
        let bad_json = r#"{"not_valid": true}"#; // missing required fields

        let good_id: &str = &"ee".repeat(32);
        let bad_id: &str = &"ff".repeat(32);

        insert_peer_entries(&mgr, "fest-3", &[(good_id, good_json), (bad_id, bad_json)]);

        let peers = mgr.parse_peer_list("fest-3", "not-me");
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].user_id, "good-user");
    }
}
