use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};

/// Generate a random 32-byte group key.
pub fn generate_group_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key).expect("getrandom failed");
    key
}

/// Encrypt plaintext with AES-256-GCM.
/// Output format: 12-byte nonce || ciphertext.
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("invalid key: {e}"))?;
    let mut nonce_bytes = [0u8; 12];
    getrandom::getrandom(&mut nonce_bytes).map_err(|e| anyhow::anyhow!("rng error: {e}"))?;
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encryption failed: {e}"))?;
    let mut out = Vec::with_capacity(12 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt AES-256-GCM ciphertext. Input format: 12-byte nonce || ciphertext.
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
    if ciphertext.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ct) = ciphertext.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("invalid key: {e}"))?;
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ct)
        .map_err(|e| anyhow::anyhow!("decryption failed: {e}"))
}

/// Derive a stable group ID string from a group key.
/// Returns the first 16 bytes of blake3(key) as a 32-char hex string.
pub fn group_id_from_key(key: &[u8; 32]) -> String {
    let hash = blake3::hash(key);
    let bytes = hash.as_bytes();
    // Format first 16 bytes as hex (32 chars)
    bytes[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = generate_group_key();
        let plaintext = b"hello offbeat";
        let encrypted = encrypt(&key, plaintext).unwrap();
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
}
