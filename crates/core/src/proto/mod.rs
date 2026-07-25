//! Protocol buffer generated types and conversion layer.
//!
//! Proto types are the **wire format** (serialization boundary).
//! Internal domain types (`types.rs`) remain for DB persistence and FRB bridge.
//! Thin `From`/`Into` impls convert between them at the boundary.

#[allow(clippy::all, clippy::pedantic)]
mod offbeat_v1 {
    include!("offbeat.v1.rs");
}

pub use offbeat_v1::*;

// ---------------------------------------------------------------------------
// Conversions: proto <-> domain types
// ---------------------------------------------------------------------------

impl From<crate::types::ChatMessage> for ChatMessage {
    fn from(m: crate::types::ChatMessage) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            display_name: m.display_name,
            text: m.text,
            topic: m.topic,
            stage_id: m.stage_id,
            timestamp: m.timestamp,
            writer_seq: m.writer_seq,
        }
    }
}

impl From<ChatMessage> for crate::types::ChatMessage {
    fn from(m: ChatMessage) -> Self {
        Self {
            id: m.id,
            user_id: m.user_id,
            display_name: m.display_name,
            text: m.text,
            topic: m.topic,
            stage_id: m.stage_id,
            timestamp: m.timestamp,
            writer_seq: m.writer_seq,
        }
    }
}

// ---------------------------------------------------------------------------
// Conversions: GossipMessage <-> GossipEnvelope
// ---------------------------------------------------------------------------

use crate::gossip_manager::GossipMessage;

impl GossipEnvelope {
    /// Convert from domain GossipMessage into a wire-ready GossipEnvelope.
    pub fn from_gossip_message(msg: &GossipMessage) -> Self {
        use gossip_envelope::Payload;
        let payload = match msg {
            GossipMessage::FestivalUpdate {
                doc_id,
                kind,
                authority_seq,
                signed_update,
            } => Payload::FestivalUpdate(FestivalUpdate {
                doc_id: doc_id.clone(),
                kind: *kind,
                authority_seq: *authority_seq,
                signed_update: Some(SignedUpdate {
                    update: signed_update.update.clone(),
                    author: signed_update.author.clone(),
                    signature: signed_update.signature.clone(),
                }),
            }),
            GossipMessage::GroupUpdate {
                doc_id,
                encrypted,
                group_key,
            } => Payload::GroupUpdate(GroupUpdate {
                doc_id: doc_id.clone(),
                encrypted: encrypted.clone(),
                group_key_id: crate::crypto::group_id_from_key(group_key),
            }),
            GossipMessage::Chat(chat) => Payload::Chat(chat.clone().into()),
            GossipMessage::EncryptedChat {
                group_key,
                encrypted,
            } => Payload::EncryptedChat(EncryptedPayload {
                encrypted: encrypted.clone(),
                group_key_id: crate::crypto::group_id_from_key(group_key),
            }),
            GossipMessage::SyncRequest {
                doc_id,
                encrypted_sv,
                group_key,
            } => Payload::SyncRequest(SyncRequest {
                doc_id: doc_id.clone(),
                encrypted_sv: encrypted_sv.clone(),
                group_key_id: crate::crypto::group_id_from_key(group_key),
            }),
            GossipMessage::SyncResponse {
                doc_id,
                encrypted_diff,
                group_key,
            } => Payload::SyncResponse(SyncResponse {
                doc_id: doc_id.clone(),
                encrypted_diff: encrypted_diff.clone(),
                group_key_id: crate::crypto::group_id_from_key(group_key),
            }),
            GossipMessage::SyncUpdate {
                doc_id,
                encrypted_diff,
                group_key,
            } => Payload::SyncUpdate(SyncUpdate {
                doc_id: doc_id.clone(),
                encrypted_diff: encrypted_diff.clone(),
                group_key_id: crate::crypto::group_id_from_key(group_key),
            }),
        };
        GossipEnvelope {
            payload: Some(payload),
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

use prost::Message;

/// Encode a GossipEnvelope to protobuf bytes.
pub fn encode_envelope(envelope: &GossipEnvelope) -> Vec<u8> {
    envelope.encode_to_vec()
}

/// Decode a GossipEnvelope from protobuf bytes.
pub fn decode_envelope(bytes: &[u8]) -> anyhow::Result<GossipEnvelope> {
    GossipEnvelope::decode(bytes).map_err(|e| anyhow::anyhow!("protobuf decode error: {e}"))
}

/// Encode a RelayClientMessage to protobuf bytes.
pub fn encode_client_msg(msg: &RelayClientMessage) -> Vec<u8> {
    msg.encode_to_vec()
}

/// Decode a RelayClientMessage from protobuf bytes.
pub fn decode_client_msg(bytes: &[u8]) -> anyhow::Result<RelayClientMessage> {
    RelayClientMessage::decode(bytes)
        .map_err(|e| anyhow::anyhow!("protobuf decode client msg: {e}"))
}

/// Encode a RelayServerMessage to protobuf bytes.
pub fn encode_server_msg(msg: &RelayServerMessage) -> Vec<u8> {
    msg.encode_to_vec()
}

/// Decode a RelayServerMessage from protobuf bytes.
pub fn decode_server_msg(bytes: &[u8]) -> anyhow::Result<RelayServerMessage> {
    RelayServerMessage::decode(bytes)
        .map_err(|e| anyhow::anyhow!("protobuf decode server msg: {e}"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types;

    #[test]
    fn test_chat_message_roundtrip() {
        let domain = types::ChatMessage {
            id: "m1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "hello".to_string(),
            topic: "festival/f1".to_string(),
            stage_id: Some("main".to_string()),
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            writer_seq: 42,
        };

        let proto: ChatMessage = domain.clone().into();
        let back: types::ChatMessage = proto.into();
        assert_eq!(back.id, domain.id);
        assert_eq!(back.stage_id, domain.stage_id);
        assert_eq!(back.writer_seq, domain.writer_seq);
    }

    #[test]
    fn test_gossip_envelope_festival_update_roundtrip() {
        let msg = GossipMessage::FestivalUpdate {
            doc_id: "festival/test/state".to_string(),
            kind: 2,
            authority_seq: 4,
            signed_update: types::SignedUpdate {
                update: b"yrs-update".to_vec(),
                author: "organiser".to_string(),
                signature: b"sig-bytes".to_vec(),
            },
        };

        let envelope = GossipEnvelope::from_gossip_message(&msg);
        let bytes = encode_envelope(&envelope);
        let decoded = decode_envelope(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn test_gossip_envelope_chat_roundtrip() {
        let chat = types::ChatMessage {
            id: "c1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Bob".to_string(),
            text: "hi".to_string(),
            topic: "t".to_string(),
            stage_id: None,
            timestamp: "now".to_string(),
            writer_seq: 1,
        };
        let msg = GossipMessage::Chat(chat);
        let envelope = GossipEnvelope::from_gossip_message(&msg);
        let bytes = encode_envelope(&envelope);
        let decoded = decode_envelope(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn test_gossip_envelope_group_update_roundtrip() {
        let key = crate::crypto::generate_group_key();
        let msg = GossipMessage::GroupUpdate {
            doc_id: "group/test".to_string(),
            encrypted: vec![1, 2, 3, 4],
            group_key: key,
        };
        let envelope = GossipEnvelope::from_gossip_message(&msg);
        let bytes = encode_envelope(&envelope);
        let decoded = decode_envelope(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn test_gossip_envelope_encrypted_chat_roundtrip() {
        let key = crate::crypto::generate_group_key();
        let msg = GossipMessage::EncryptedChat {
            group_key: key,
            encrypted: vec![10, 20, 30],
        };
        let envelope = GossipEnvelope::from_gossip_message(&msg);
        let bytes = encode_envelope(&envelope);
        let decoded = decode_envelope(&bytes).unwrap();
        assert_eq!(decoded, envelope);
    }

    #[test]
    fn test_gossip_envelope_sync_variants_roundtrip() {
        let key = crate::crypto::generate_group_key();

        for msg in [
            GossipMessage::SyncRequest {
                doc_id: "doc".to_string(),
                encrypted_sv: vec![1, 2],
                group_key: key,
            },
            GossipMessage::SyncResponse {
                doc_id: "doc".to_string(),
                encrypted_diff: vec![3, 4],
                group_key: key,
            },
            GossipMessage::SyncUpdate {
                doc_id: "doc".to_string(),
                encrypted_diff: vec![5, 6],
                group_key: key,
            },
        ] {
            let envelope = GossipEnvelope::from_gossip_message(&msg);
            let bytes = encode_envelope(&envelope);
            let decoded = decode_envelope(&bytes).unwrap();
            assert_eq!(decoded, envelope);
        }
    }

    #[test]
    fn test_relay_client_message_roundtrip() {
        let msg = RelayClientMessage {
            msg: Some(relay_client_message::Msg::Subscribe(SubscribeRequest {
                topics: vec!["topic1".to_string(), "topic2".to_string()],
            })),
        };
        let bytes = encode_client_msg(&msg);
        let decoded = decode_client_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_relay_server_message_roundtrip() {
        let msg = RelayServerMessage {
            msg: Some(relay_server_message::Msg::AuthOk(AuthOk {
                authenticated: true,
                admin_count: 2,
            })),
        };
        let bytes = encode_server_msg(&msg);
        let decoded = decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn test_relay_error_roundtrip() {
        let msg = RelayServerMessage {
            msg: Some(relay_server_message::Msg::Error(RelayError {
                error: "bad request".to_string(),
                code: ErrorCode::Malformed as i32,
            })),
        };
        let bytes = encode_server_msg(&msg);
        let decoded = decode_server_msg(&bytes).unwrap();
        assert_eq!(decoded, msg);
    }
}
