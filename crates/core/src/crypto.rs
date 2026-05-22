use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use chacha20poly1305::XChaCha20Poly1305;

// ---------------------------------------------------------------------------
// Envelope version tags
// ---------------------------------------------------------------------------

/// AES-256-GCM with 12-byte random nonce.
const ENVELOPE_V1: u8 = 0x01;
/// XChaCha20-Poly1305 with 24-byte random nonce.
const ENVELOPE_V2: u8 = 0x02;

// ---------------------------------------------------------------------------
// Key generation
// ---------------------------------------------------------------------------

/// Generate a random 32-byte group key.
pub fn generate_group_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key).expect("getrandom failed");
    key
}

// ---------------------------------------------------------------------------
// Encrypt — always uses V2 (XChaCha20-Poly1305)
// ---------------------------------------------------------------------------

/// Encrypt plaintext with XChaCha20-Poly1305.
/// Output format: `[version(1) || nonce(24) || ciphertext]`.
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("invalid key: {e}"))?;
    let mut nonce_bytes = [0u8; 24];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| anyhow::anyhow!("rng error: {e}"))?;
    let nonce = chacha20poly1305::XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    let mut out = Vec::with_capacity(1 + 24 + ciphertext.len());
    out.push(ENVELOPE_V2);
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Decrypt — supports both V1 and V2
// ---------------------------------------------------------------------------

/// Decrypt ciphertext produced by `encrypt` or legacy V0/V1 format.
///
/// Detects the envelope version from the first byte:
/// - `0x01` → AES-256-GCM, 12-byte nonce
/// - `0x02` → XChaCha20-Poly1305, 24-byte nonce
/// - Other  → Legacy (no version byte): AES-256-GCM, 12-byte nonce
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
    if ciphertext.is_empty() {
        anyhow::bail!("ciphertext is empty");
    }

    match ciphertext[0] {
        ENVELOPE_V1 => decrypt_v1(key, &ciphertext[1..]),
        ENVELOPE_V2 => decrypt_v2(key, &ciphertext[1..]),
        _ => {
            // Legacy format (no version byte): AES-256-GCM with 12-byte nonce
            decrypt_v1(key, ciphertext)
        }
    }
}

/// AES-256-GCM decryption. Input: `[nonce(12) || ciphertext]`.
fn decrypt_v1(key: &[u8; 32], data: &[u8]) -> anyhow::Result<Vec<u8>> {
    if data.len() < 12 {
        anyhow::bail!("V1 ciphertext too short");
    }
    let (nonce_bytes, ct) = data.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("invalid key: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("V1 decryption failed: {e}"))
}

/// XChaCha20-Poly1305 decryption. Input: `[nonce(24) || ciphertext]`.
fn decrypt_v2(key: &[u8; 32], data: &[u8]) -> anyhow::Result<Vec<u8>> {
    if data.len() < 24 {
        anyhow::bail!("V2 ciphertext too short");
    }
    let (nonce_bytes, ct) = data.split_at(24);
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("invalid key: {e}"))?;
    let nonce = chacha20poly1305::XNonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("V2 decryption failed: {e}"))
}

// ---------------------------------------------------------------------------
// Group ID derivation
// ---------------------------------------------------------------------------

/// Derive a stable group ID string from a group key.
/// Returns the first 16 bytes of blake3(key) as a 32-char hex string.
pub fn group_id_from_key(key: &[u8; 32]) -> String {
    let hash = blake3::hash(key);
    let bytes = hash.as_bytes();
    bytes[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_group_key();
        let plaintext = b"hello offbeat";
        let encrypted = encrypt(&key, plaintext).unwrap();
        // Should start with V2 version byte
        assert_eq!(encrypted[0], ENVELOPE_V2);
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key = generate_group_key();
        let wrong_key = generate_group_key();
        let encrypted = encrypt(&key, b"secret data").unwrap();
        let result = decrypt(&wrong_key, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_legacy_v0_format() {
        // Legacy format: [nonce(12) || ciphertext] with AES-256-GCM, no version byte
        let key = generate_group_key();
        let plaintext = b"legacy data";

        // Manually produce legacy format
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut nonce_bytes = [0u8; 12];
        getrandom::getrandom(&mut nonce_bytes).unwrap();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher.encrypt(nonce, plaintext.as_ref()).unwrap();
        let mut legacy = Vec::with_capacity(12 + ct.len());
        legacy.extend_from_slice(&nonce_bytes);
        legacy.extend_from_slice(&ct);

        // Decrypt should handle legacy format
        let decrypted = decrypt(&key, &legacy).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_v1_explicit() {
        // V1 format: [0x01 || nonce(12) || ciphertext]
        let key = generate_group_key();
        let plaintext = b"v1 data";

        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut nonce_bytes = [0u8; 12];
        getrandom::getrandom(&mut nonce_bytes).unwrap();
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ct = cipher.encrypt(nonce, plaintext.as_ref()).unwrap();
        let mut v1 = Vec::with_capacity(1 + 12 + ct.len());
        v1.push(ENVELOPE_V1);
        v1.extend_from_slice(&nonce_bytes);
        v1.extend_from_slice(&ct);

        let decrypted = decrypt(&key, &v1).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_group_id_deterministic() {
        let key = generate_group_key();
        let id1 = group_id_from_key(&key);
        let id2 = group_id_from_key(&key);
        assert_eq!(id1, id2);
        assert_eq!(id1.len(), 32);
    }

    #[test]
    fn test_group_id_different_keys() {
        let key1 = generate_group_key();
        let key2 = generate_group_key();
        let id1 = group_id_from_key(&key1);
        let id2 = group_id_from_key(&key2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_empty_ciphertext_fails() {
        let key = generate_group_key();
        assert!(decrypt(&key, &[]).is_err());
    }

    #[test]
    fn test_v2_nonce_is_24_bytes() {
        let key = generate_group_key();
        let encrypted = encrypt(&key, b"test").unwrap();
        // V2: [version(1) || nonce(24) || ciphertext(16+4)]
        assert_eq!(encrypted[0], ENVELOPE_V2);
        assert!(encrypted.len() > 1 + 24);
    }
}
