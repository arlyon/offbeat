use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

const FESTIVAL_UPDATE_DOMAIN: &[u8] = b"offbeat/festival-update/v1\0";
const PUBLIC_CHAT_DOMAIN: &[u8] = b"offbeat/public-chat/v1\0";
const RELAY_AUTH_DOMAIN: &[u8] = b"offbeat/relay-auth/v1\0";

pub fn relay_auth_signing_payload(
    festival_id: &str,
    challenge: &[u8],
    public_key: &[u8],
) -> anyhow::Result<Vec<u8>> {
    if festival_id.is_empty() {
        anyhow::bail!("festival ID is required");
    }
    if challenge.len() != 32 {
        anyhow::bail!("relay auth challenge must be 32 bytes");
    }
    if public_key.len() != 32 {
        anyhow::bail!("relay auth public key must be 32 bytes");
    }
    let festival_id = festival_id.as_bytes();
    let festival_len = u16::try_from(festival_id.len())
        .map_err(|_| anyhow::anyhow!("festival ID is too large"))?;
    let mut payload = Vec::with_capacity(RELAY_AUTH_DOMAIN.len() + festival_id.len() + 70);
    payload.extend_from_slice(RELAY_AUTH_DOMAIN);
    payload.extend_from_slice(&festival_len.to_be_bytes());
    payload.extend_from_slice(festival_id);
    payload.extend_from_slice(&(challenge.len() as u16).to_be_bytes());
    payload.extend_from_slice(challenge);
    payload.extend_from_slice(&(public_key.len() as u16).to_be_bytes());
    payload.extend_from_slice(public_key);
    Ok(payload)
}

fn append_len_prefixed(payload: &mut Vec<u8>, value: &[u8], field: &str) -> anyhow::Result<()> {
    let len = u32::try_from(value.len())
        .map_err(|_| anyhow::anyhow!("public chat {field} is too large"))?;
    payload.extend_from_slice(&len.to_be_bytes());
    payload.extend_from_slice(value);
    Ok(())
}

fn public_chat_channel(topic: &str) -> Option<&str> {
    let mut parts = topic.split('/');
    match (
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
        parts.next(),
    ) {
        (Some("festival"), Some(festival_id), Some("chat"), Some(channel), None)
            if !festival_id.is_empty() && !channel.is_empty() =>
        {
            Some(channel)
        }
        _ => None,
    }
}

/// Canonical bytes signed by the author of a public campsite or stage message.
///
/// Length prefixes and an explicit optional-stage marker make the encoding
/// unambiguous across Rust, TypeScript, and constrained transports.
pub fn public_chat_signing_payload(message: &crate::types::ChatMessage) -> anyhow::Result<Vec<u8>> {
    if message.writer_seq == 0 || message.logical_time == 0 {
        anyhow::bail!("public chat message is missing its append-log position");
    }
    if message.writer_key.len() != 32 {
        anyhow::bail!("public chat writer key must be 32 bytes");
    }
    let Some(channel) = public_chat_channel(&message.topic) else {
        anyhow::bail!("invalid public chat topic");
    };
    match message.stage_id.as_deref() {
        Some(stage_id) if stage_id == channel => {}
        None if channel == "campsite" => {}
        _ => anyhow::bail!("public chat stage does not match its topic"),
    }

    let expected_user_id: String = message.writer_key[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if message.user_id != expected_user_id {
        anyhow::bail!("public chat user ID does not match its writer key");
    }

    let mut payload = Vec::with_capacity(PUBLIC_CHAT_DOMAIN.len() + message.text.len() + 256);
    payload.extend_from_slice(PUBLIC_CHAT_DOMAIN);
    append_len_prefixed(&mut payload, message.id.as_bytes(), "message ID")?;
    append_len_prefixed(&mut payload, message.user_id.as_bytes(), "user ID")?;
    append_len_prefixed(
        &mut payload,
        message.display_name.as_bytes(),
        "display name",
    )?;
    append_len_prefixed(&mut payload, message.text.as_bytes(), "text")?;
    append_len_prefixed(&mut payload, message.topic.as_bytes(), "topic")?;
    match &message.stage_id {
        Some(stage_id) => {
            payload.push(1);
            append_len_prefixed(&mut payload, stage_id.as_bytes(), "stage ID")?;
        }
        None => payload.push(0),
    }
    append_len_prefixed(&mut payload, message.timestamp.as_bytes(), "timestamp")?;
    payload.extend_from_slice(&message.writer_seq.to_be_bytes());
    payload.extend_from_slice(&message.logical_time.to_be_bytes());
    append_len_prefixed(&mut payload, &message.writer_key, "writer key")?;
    Ok(payload)
}

pub fn sign_public_chat_message(
    signing_key: &SigningKey,
    message: &mut crate::types::ChatMessage,
) -> anyhow::Result<()> {
    message.writer_key = signing_key.verifying_key().to_bytes().to_vec();
    message.signature = sign(signing_key, &public_chat_signing_payload(message)?);
    Ok(())
}

pub fn verify_public_chat_message(message: &crate::types::ChatMessage) -> bool {
    let Ok(writer_key) = <[u8; 32]>::try_from(message.writer_key.as_slice()) else {
        return false;
    };
    public_chat_signing_payload(message)
        .is_ok_and(|payload| verify(&writer_key, &payload, &message.signature))
}

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
    fn public_chat_signature_matches_typescript_vector() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let mut message = crate::types::ChatMessage {
            id: "message-1".to_string(),
            user_id: "ea4a6c63e29c520a".to_string(),
            display_name: "Alice".to_string(),
            text: "Meet by the sound desk".to_string(),
            topic: "festival/fieldday/chat/main-stage".to_string(),
            stage_id: Some("main-stage".to_string()),
            timestamp: "2026-06-14T20:00:00Z".to_string(),
            writer_seq: 7,
            logical_time: 42,
            writer_key: Vec::new(),
            signature: Vec::new(),
            trust: crate::types::ChatTrust::Unverified,
        };
        sign_public_chat_message(&key, &mut message).unwrap();

        assert_eq!(
            hex::encode(public_chat_signing_payload(&message).unwrap()),
            "6f6666626561742f7075626c69632d636861742f763100000000096d6573736167652d31000000106561346136633633653239633532306100000005416c696365000000164d6565742062792074686520736f756e64206465736b00000021666573746976616c2f6669656c646461792f636861742f6d61696e2d7374616765010000000a6d61696e2d737461676500000014323032362d30362d31345432303a30303a30305a0000000000000007000000000000002a00000020ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
        );
        assert_eq!(
            hex::encode(&message.signature),
            "6a9cdb6087a466b25b45df94e7fb45ab6804295b709c0ea0c77ea17178be6d1a08e5e612f61518c546b0d436a2d5abd06727e20f4eab561eb8de074a72222c0e"
        );
        assert!(verify_public_chat_message(&message));

        message.text.push('!');
        assert!(!verify_public_chat_message(&message));
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
