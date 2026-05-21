use ed25519_dalek::SigningKey;

use crate::db::Database;

/// Generate a new identity signing key, or load the existing one from the
/// credentials table.
pub fn generate_or_load_identity(db: &Database) -> anyhow::Result<SigningKey> {
    if let Some(bytes) = db.get_credential("identity_secret_key")? {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("stored identity key has wrong length"))?;
        return Ok(SigningKey::from_bytes(&arr));
    }

    // Generate a fresh key.
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| anyhow::anyhow!("rng error: {e}"))?;
    let key = SigningKey::from_bytes(&seed);

    db.set_credential("identity_secret_key", &seed)?;
    Ok(key)
}

/// Derive a short, stable user ID from a signing key.
///
/// Returns the first 16 hex characters of the verifying (public) key — a 64-bit
/// prefix that is unique enough for local use.
pub fn get_user_id(signing_key: &SigningKey) -> String {
    let vk_bytes = signing_key.verifying_key().to_bytes();
    vk_bytes[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Read the display name from the credentials table.
pub fn get_display_name(db: &Database) -> anyhow::Result<Option<String>> {
    match db.get_credential("display_name")? {
        Some(bytes) => {
            let s = String::from_utf8(bytes)
                .map_err(|e| anyhow::anyhow!("invalid display name encoding: {e}"))?;
            Ok(Some(s))
        }
        None => Ok(None),
    }
}

/// Persist a display name to the credentials table.
pub fn set_display_name(db: &Database, name: &str) -> anyhow::Result<()> {
    db.set_credential("display_name", name.as_bytes())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    #[test]
    fn test_generate_identity() {
        let db = test_db();
        let key = generate_or_load_identity(&db).unwrap();
        let user_id = get_user_id(&key);
        assert_eq!(user_id.len(), 16);
        // The key was persisted.
        let raw = db.get_credential("identity_secret_key").unwrap();
        assert!(raw.is_some());
    }

    #[test]
    fn test_load_existing_identity() {
        let db = test_db();
        let key1 = generate_or_load_identity(&db).unwrap();
        let key2 = generate_or_load_identity(&db).unwrap();
        assert_eq!(
            key1.verifying_key().to_bytes(),
            key2.verifying_key().to_bytes()
        );
    }

    #[test]
    fn test_user_id_deterministic() {
        let db = test_db();
        let key = generate_or_load_identity(&db).unwrap();
        let id1 = get_user_id(&key);
        let id2 = get_user_id(&key);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 16);
    }

    #[test]
    fn test_display_name_crud() {
        let db = test_db();
        assert_eq!(get_display_name(&db).unwrap(), None);

        set_display_name(&db, "Alice").unwrap();
        assert_eq!(get_display_name(&db).unwrap(), Some("Alice".to_string()));

        set_display_name(&db, "Alice B.").unwrap();
        assert_eq!(
            get_display_name(&db).unwrap(),
            Some("Alice B.".to_string())
        );
    }
}
