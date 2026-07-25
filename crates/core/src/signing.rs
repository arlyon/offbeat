use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

const FESTIVAL_UPDATE_DOMAIN: &[u8] = b"offbeat/festival-update/v1\0";

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

/// Canonical bytes signed by the festival authority for a checkpoint or delta.
///
/// Binding the document, representation kind, and authority sequence prevents a
/// valid update from being replayed under another festival or envelope type.
pub fn festival_update_signing_payload(
    doc_id: &str,
    kind: i32,
    authority_seq: u64,
    update: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if !matches!(kind, 1 | 2) {
        anyhow::bail!("invalid festival update kind {kind}");
    }
    let doc_id = doc_id.as_bytes();
    let doc_len = u16::try_from(doc_id.len())
        .map_err(|_| anyhow::anyhow!("festival document ID is too long"))?;
    let update_len =
        u32::try_from(update.len()).map_err(|_| anyhow::anyhow!("festival update is too large"))?;

    let mut payload = Vec::with_capacity(
        FESTIVAL_UPDATE_DOMAIN.len() + 2 + doc_id.len() + 1 + 8 + 4 + update.len(),
    );
    payload.extend_from_slice(FESTIVAL_UPDATE_DOMAIN);
    payload.extend_from_slice(&doc_len.to_be_bytes());
    payload.extend_from_slice(doc_id);
    payload.push(kind as u8);
    payload.extend_from_slice(&authority_seq.to_be_bytes());
    payload.extend_from_slice(&update_len.to_be_bytes());
    payload.extend_from_slice(update);
    Ok(payload)
}

pub fn sign_festival_update(
    signing_key: &SigningKey,
    doc_id: &str,
    kind: i32,
    authority_seq: u64,
    update: &[u8],
) -> anyhow::Result<Vec<u8>> {
    Ok(sign(
        signing_key,
        &festival_update_signing_payload(doc_id, kind, authority_seq, update)?,
    ))
}

pub fn verify_festival_update(
    public_key_bytes: &[u8; 32],
    doc_id: &str,
    kind: i32,
    authority_seq: u64,
    update: &[u8],
    signature_bytes: &[u8],
) -> bool {
    festival_update_signing_payload(doc_id, kind, authority_seq, update)
        .is_ok_and(|payload| verify(public_key_bytes, &payload, signature_bytes))
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

    #[test]
    fn festival_signature_binds_context() {
        let key = generate_signing_key();
        let public_key = key.verifying_key().to_bytes();
        let update = b"yrs-update";
        let signature = sign_festival_update(&key, "festival/a/state", 2, 7, update).unwrap();

        assert!(verify_festival_update(
            &public_key,
            "festival/a/state",
            2,
            7,
            update,
            &signature,
        ));
        assert!(!verify_festival_update(
            &public_key,
            "festival/b/state",
            2,
            7,
            update,
            &signature,
        ));
        assert!(!verify_festival_update(
            &public_key,
            "festival/a/state",
            1,
            7,
            update,
            &signature,
        ));
        assert!(!verify_festival_update(
            &public_key,
            "festival/a/state",
            2,
            8,
            update,
            &signature,
        ));
    }
}
