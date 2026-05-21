use ed25519_dalek::SigningKey;
use hkdf::Hkdf;
use sha2::Sha256;

use crate::db::Database;
use crate::signing;

// ---------------------------------------------------------------------------
// Identity
// ---------------------------------------------------------------------------

/// Generate a new identity signing key, or load the existing one from the
/// credentials table.
pub fn generate_or_load_identity(db: &Database) -> anyhow::Result<SigningKey> {
    if let Some(bytes) = db.get_credential("identity_secret_key")? {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("stored identity key has wrong length"))?;
        return Ok(SigningKey::from_bytes(&arr));
    }

    // Generate a fresh key (fallback when PRF is not available).
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|e| anyhow::anyhow!("rng error: {e}"))?;
    let key = SigningKey::from_bytes(&seed);

    db.set_credential("identity_secret_key", &seed)?;
    Ok(key)
}

/// Derive an Ed25519 signing key from a WebAuthn PRF output.
///
/// Uses HKDF-SHA256 to derive a 32-byte Ed25519 seed from the PRF output.
/// Same PRF output always produces the same key — deterministic identity
/// recovery across devices.
pub fn derive_identity_from_prf(db: &Database, prf_output: &[u8; 32]) -> anyhow::Result<SigningKey> {
    let hk = Hkdf::<Sha256>::new(Some(b"offbeat"), prf_output);
    let mut seed = [0u8; 32];
    hk.expand(b"ed25519-identity", &mut seed)
        .map_err(|e| anyhow::anyhow!("HKDF expand failed: {e}"))?;

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

/// Return the full hex-encoded Ed25519 public key (64 chars).
pub fn get_public_key_hex(signing_key: &SigningKey) -> String {
    let vk_bytes = signing_key.verifying_key().to_bytes();
    vk_bytes.iter().map(|b| format!("{b:02x}")).collect()
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
// Attestation
// ---------------------------------------------------------------------------

/// A MainDO-signed attestation binding an Ed25519 public key to a WebAuthn
/// registration. Portable proof of identity.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Attestation {
    /// The signed message: "attestation:v1:<pubkey_hex>:<issued_unix>:<expires_unix>"
    pub message: String,
    /// Hex-encoded Ed25519 signature over the message bytes.
    pub signature: String,
    /// Hex-encoded Ed25519 public key of the issuer (MainDO).
    pub issuer: String,
}

/// Current auth state of the local identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthState {
    /// No attestation stored — needs WebAuthn registration.
    Unregistered,
    /// Attestation is valid.
    Valid,
    /// Attestation expires within the given number of days.
    Expiring(u32),
    /// Attestation has expired (may still be within grace period).
    Expired,
}

/// Store an attestation in the local credentials table.
pub fn store_attestation(db: &Database, attestation: &Attestation) -> anyhow::Result<()> {
    db.set_credential("attestation_message", attestation.message.as_bytes())?;
    db.set_credential("attestation_signature", attestation.signature.as_bytes())?;
    db.set_credential("attestation_issuer", attestation.issuer.as_bytes())?;
    Ok(())
}

/// Load the stored attestation, if any.
pub fn load_attestation(db: &Database) -> anyhow::Result<Option<Attestation>> {
    let Some(message) = db.get_credential("attestation_message")? else {
        return Ok(None);
    };
    let Some(signature) = db.get_credential("attestation_signature")? else {
        return Ok(None);
    };
    let Some(issuer) = db.get_credential("attestation_issuer")? else {
        return Ok(None);
    };
    Ok(Some(Attestation {
        message: String::from_utf8(message)?,
        signature: String::from_utf8(signature)?,
        issuer: String::from_utf8(issuer)?,
    }))
}

/// Determine the current auth state from the stored attestation.
pub fn attestation_state(db: &Database) -> anyhow::Result<AuthState> {
    let Some(att) = load_attestation(db)? else {
        return Ok(AuthState::Unregistered);
    };
    let expires_at = parse_attestation_expiry(&att.message)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    if now > expires_at {
        return Ok(AuthState::Expired);
    }
    let days_left = ((expires_at - now) / 86400) as u32;
    if days_left <= 7 {
        Ok(AuthState::Expiring(days_left))
    } else {
        Ok(AuthState::Valid)
    }
}

/// Verify an attestation's signature against a known MainDO public key.
pub fn verify_attestation(attestation: &Attestation, main_do_pubkey: &[u8; 32]) -> bool {
    let sig_bytes = match hex_to_bytes(&attestation.signature) {
        Some(b) => b,
        None => return false,
    };
    signing::verify(main_do_pubkey, attestation.message.as_bytes(), &sig_bytes)
}

/// Parse the expiry timestamp from an attestation message.
fn parse_attestation_expiry(message: &str) -> anyhow::Result<u64> {
    // Format: "attestation:v1:<pubkey>:<issued>:<expires>"
    let parts: Vec<&str> = message.split(':').collect();
    if parts.len() != 5 {
        anyhow::bail!("invalid attestation message format");
    }
    parts[4]
        .parse::<u64>()
        .map_err(|e| anyhow::anyhow!("invalid expiry timestamp: {e}"))
}

fn hex_to_bytes(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
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

    #[test]
    fn test_prf_derivation_deterministic() {
        let db = test_db();
        let prf_output = [42u8; 32];
        let key1 = derive_identity_from_prf(&db, &prf_output).unwrap();
        // Load again from DB — should be the same.
        let key2 = generate_or_load_identity(&db).unwrap();
        assert_eq!(
            key1.verifying_key().to_bytes(),
            key2.verifying_key().to_bytes()
        );
    }

    #[test]
    fn test_prf_derivation_different_inputs() {
        let db1 = test_db();
        let db2 = test_db();
        let key1 = derive_identity_from_prf(&db1, &[1u8; 32]).unwrap();
        let key2 = derive_identity_from_prf(&db2, &[2u8; 32]).unwrap();
        assert_ne!(
            key1.verifying_key().to_bytes(),
            key2.verifying_key().to_bytes()
        );
    }

    #[test]
    fn test_public_key_hex() {
        let db = test_db();
        let key = generate_or_load_identity(&db).unwrap();
        let hex = get_public_key_hex(&key);
        assert_eq!(hex.len(), 64);
    }

    #[test]
    fn test_attestation_roundtrip() {
        let db = test_db();
        let att = Attestation {
            message: "attestation:v1:abcd:1000000:2000000".to_string(),
            signature: "deadbeef".to_string(),
            issuer: "cafebabe".to_string(),
        };
        store_attestation(&db, &att).unwrap();
        let loaded = load_attestation(&db).unwrap().unwrap();
        assert_eq!(loaded.message, att.message);
        assert_eq!(loaded.signature, att.signature);
        assert_eq!(loaded.issuer, att.issuer);
    }

    #[test]
    fn test_attestation_state_unregistered() {
        let db = test_db();
        assert_eq!(attestation_state(&db).unwrap(), AuthState::Unregistered);
    }

    #[test]
    fn test_attestation_state_valid() {
        let db = test_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let att = Attestation {
            message: format!("attestation:v1:abcd:{now}:{}", now + 30 * 86400),
            signature: "aa".to_string(),
            issuer: "bb".to_string(),
        };
        store_attestation(&db, &att).unwrap();
        assert_eq!(attestation_state(&db).unwrap(), AuthState::Valid);
    }

    #[test]
    fn test_attestation_state_expiring() {
        let db = test_db();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let att = Attestation {
            message: format!("attestation:v1:abcd:{now}:{}", now + 3 * 86400),
            signature: "aa".to_string(),
            issuer: "bb".to_string(),
        };
        store_attestation(&db, &att).unwrap();
        assert!(matches!(attestation_state(&db).unwrap(), AuthState::Expiring(3)));
    }

    #[test]
    fn test_verify_attestation() {
        let signing_key = crate::signing::generate_signing_key();
        let pubkey = signing_key.verifying_key().to_bytes();
        let message = "attestation:v1:abcd:1000:2000";
        let sig = crate::signing::sign(&signing_key, message.as_bytes());
        let sig_hex: String = sig.iter().map(|b| format!("{b:02x}")).collect();
        let issuer_hex: String = pubkey.iter().map(|b| format!("{b:02x}")).collect();

        let att = Attestation {
            message: message.to_string(),
            signature: sig_hex,
            issuer: issuer_hex,
        };
        assert!(verify_attestation(&att, &pubkey));
    }
}
