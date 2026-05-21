use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

/// Generate a new random Ed25519 signing key.
pub fn generate_signing_key() -> SigningKey {
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).expect("getrandom failed");
    SigningKey::from_bytes(&seed)
}

/// Sign data with the given signing key. Returns signature bytes.
pub fn sign(signing_key: &SigningKey, data: &[u8]) -> Vec<u8> {
    let sig: Signature = signing_key.sign(data);
    sig.to_bytes().to_vec()
}

/// Verify an Ed25519 signature. Returns false on any error.
pub fn verify(public_key_bytes: &[u8; 32], data: &[u8], signature_bytes: &[u8]) -> bool {
    let Ok(verifying_key) = VerifyingKey::from_bytes(public_key_bytes) else {
        return false;
    };
    let Ok(sig_array): Result<[u8; 64], _> = signature_bytes.try_into() else {
        return false;
    };
    let signature = Signature::from_bytes(&sig_array);
    verifying_key.verify(data, &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_verify_roundtrip() {
        let key = generate_signing_key();
        let public_key: [u8; 32] = key.verifying_key().to_bytes();
        let data = b"test message for signing";
        let sig = sign(&key, data);
        assert!(verify(&public_key, data, &sig));
    }

    #[test]
    fn test_verify_wrong_data_fails() {
        let key = generate_signing_key();
        let public_key: [u8; 32] = key.verifying_key().to_bytes();
        let sig = sign(&key, b"original data");
        assert!(!verify(&public_key, b"tampered data", &sig));
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let key = generate_signing_key();
        let wrong_key = generate_signing_key();
        let wrong_public: [u8; 32] = wrong_key.verifying_key().to_bytes();
        let data = b"test message";
        let sig = sign(&key, data);
        assert!(!verify(&wrong_public, data, &sig));
    }
}
