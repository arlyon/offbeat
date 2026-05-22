//! In-memory group key cache to avoid DB lookups in the gossip hot path.

use dashmap::DashMap;

use crate::db::Database;

/// Thread-safe in-memory cache for group encryption keys.
///
/// Populated from DB on startup, updated on save_group/delete_group.
/// All gossip decode paths read from this cache instead of hitting SQLite.
pub struct GroupKeyCache {
    keys: DashMap<String, [u8; 32]>,
}

impl GroupKeyCache {
    /// Create a new empty cache.
    pub fn new() -> Self {
        Self {
            keys: DashMap::new(),
        }
    }

    /// Populate cache from the database (call on startup).
    pub fn load_from_db(&self, _db: &Database) -> anyhow::Result<()> {
        // Groups are loaded lazily on first access via get_or_load.
        Ok(())
    }

    /// Get a group key from cache.
    pub fn get(&self, group_id: &str) -> Option<[u8; 32]> {
        self.keys.get(group_id).map(|r| *r.value())
    }

    /// Insert or update a group key in cache.
    pub fn insert(&self, group_id: &str, key: [u8; 32]) {
        self.keys.insert(group_id.to_string(), key);
    }

    /// Remove a group key from cache.
    pub fn remove(&self, group_id: &str) {
        self.keys.remove(group_id);
    }

    /// Look up a group key, falling back to DB on cache miss.
    /// Inserts into cache on DB hit for future lookups.
    pub fn get_or_load(&self, group_id: &str, db: &Database) -> anyhow::Result<Option<[u8; 32]>> {
        if let Some(key) = self.get(group_id) {
            return Ok(Some(key));
        }

        // Cache miss — look up from DB
        if let Some(key) = db.load_group_key(group_id)? {
            self.insert(group_id, key);
            Ok(Some(key))
        } else {
            Ok(None)
        }
    }
}

impl Default for GroupKeyCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hit_miss() {
        let cache = GroupKeyCache::new();
        let key = [42u8; 32];

        // Miss
        assert!(cache.get("group1").is_none());

        // Insert + hit
        cache.insert("group1", key);
        assert_eq!(cache.get("group1"), Some(key));

        // Remove + miss
        cache.remove("group1");
        assert!(cache.get("group1").is_none());
    }

    #[test]
    fn test_get_or_load_from_db() {
        let db = Database::new_in_memory().unwrap();
        let cache = GroupKeyCache::new();
        let key = crate::crypto::generate_group_key();
        let group_id = crate::crypto::group_id_from_key(&key);

        // Not in cache or DB
        assert!(cache.get_or_load(&group_id, &db).unwrap().is_none());

        // Add to DB
        db.save_group(&group_id, "fest", "grp", &key).unwrap();

        // Cache miss, DB hit → should populate cache
        let loaded = cache.get_or_load(&group_id, &db).unwrap();
        assert_eq!(loaded, Some(key));

        // Now should be in cache
        assert_eq!(cache.get(&group_id), Some(key));
    }
}
