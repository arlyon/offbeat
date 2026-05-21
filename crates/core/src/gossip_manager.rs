use crate::crypto;
use crate::db::Database;
use crate::doc_manager::DocManager;
use crate::types::{ChatMessage, SignedUpdate};

/// Messages received from the gossip network.
#[derive(Debug, Clone)]
pub enum GossipMessage {
    FestivalUpdate {
        doc_id: String,
        signed_update: SignedUpdate,
    },
    GroupUpdate {
        doc_id: String,
        encrypted: Vec<u8>,
        group_key: [u8; 32],
    },
    Chat(ChatMessage),
    EncryptedChat {
        group_key: [u8; 32],
        encrypted: Vec<u8>,
    },
}

/// Dispatch an incoming gossip message to the appropriate handler.
///
/// - `FestivalUpdate` → verifies Ed25519 signature and applies the Yrs update.
/// - `GroupUpdate`    → decrypts with the group key and applies the Yrs update.
/// - `Chat`           → persists to the database.
/// - `EncryptedChat`  → decrypts with the group key, deserialises, and persists.
pub fn dispatch_message(
    doc_manager: &mut DocManager,
    db: &Database,
    msg: GossipMessage,
    festival_public_key: Option<&[u8; 32]>,
) -> anyhow::Result<()> {
    match msg {
        GossipMessage::FestivalUpdate {
            doc_id,
            signed_update,
        } => {
            let pk = festival_public_key
                .ok_or_else(|| anyhow::anyhow!("no festival public key provided"))?;
            doc_manager.apply_signed_update(&doc_id, &signed_update, pk)?;
        }

        GossipMessage::GroupUpdate {
            doc_id,
            encrypted,
            group_key,
        } => {
            doc_manager.apply_encrypted_update(&doc_id, &encrypted, &group_key)?;
        }

        GossipMessage::Chat(msg) => {
            db.save_chat_message(&msg)?;
        }

        GossipMessage::EncryptedChat {
            group_key,
            encrypted,
        } => {
            let plaintext = crypto::decrypt(&group_key, &encrypted)?;
            let chat: ChatMessage = serde_json::from_slice(&plaintext)
                .map_err(|e| anyhow::anyhow!("deserialise chat: {e}"))?;
            db.save_chat_message(&chat)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing;
    use base64::Engine as _;
    use std::sync::Arc;
    use yrs::{Doc, Map, ReadTxn, StateVector, Transact};

    fn test_db() -> Arc<Database> {
        Arc::new(Database::new_in_memory().expect("in-memory db"))
    }

    #[test]
    fn test_dispatch_chat_stored() {
        let db_arc = test_db();
        let mut doc_mgr = DocManager::new(db_arc.clone());

        let msg = ChatMessage {
            id: "m1".to_string(),
            user_id: "u1".to_string(),
            display_name: "Alice".to_string(),
            text: "hello festival!".to_string(),
            topic: "festival/f1".to_string(),
            stage_id: None,
            timestamp: "2026-06-14T20:00:00Z".to_string(),
        };

        dispatch_message(
            &mut doc_mgr,
            &db_arc,
            GossipMessage::Chat(msg.clone()),
            None,
        )
        .unwrap();

        let stored = db_arc
            .get_chat_messages("festival/f1", 10, 0)
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].text, "hello festival!");
    }

    #[test]
    fn test_dispatch_festival_update_valid_sig() {
        let db_arc = test_db();
        let mut doc_mgr = DocManager::new(db_arc.clone());

        let signing_key = signing::generate_signing_key();
        let public_key: [u8; 32] = signing_key.verifying_key().to_bytes();

        // Build a Yrs update
        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "stage", "main");
        }
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());

        let engine = base64::engine::general_purpose::STANDARD;
        let sig = signing::sign(&signing_key, &update_bytes);
        let signed = SignedUpdate {
            update: engine.encode(&update_bytes),
            author: "organiser".to_string(),
            signature: engine.encode(&sig),
        };

        dispatch_message(
            &mut doc_mgr,
            &db_arc,
            GossipMessage::FestivalUpdate {
                doc_id: "fest-doc".to_string(),
                signed_update: signed,
            },
            Some(&public_key),
        )
        .unwrap();

        let val = doc_mgr.read_map_value("fest-doc", "stage");
        assert_eq!(val, Some("main".to_string()));
    }

    #[test]
    fn test_dispatch_group_update() {
        let db_arc = test_db();
        let mut doc_mgr = DocManager::new(db_arc.clone());

        let group_key = crypto::generate_group_key();

        // Build a Yrs update
        let update_doc = Doc::new();
        let map = update_doc.get_or_insert_map("root");
        {
            let mut txn = update_doc.transact_mut();
            map.insert(&mut txn, "pin", "tent-area");
        }
        let update_bytes = update_doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());

        let encrypted = crypto::encrypt(&group_key, &update_bytes).unwrap();

        dispatch_message(
            &mut doc_mgr,
            &db_arc,
            GossipMessage::GroupUpdate {
                doc_id: "group-doc".to_string(),
                encrypted,
                group_key,
            },
            None,
        )
        .unwrap();

        let val = doc_mgr.read_map_value("group-doc", "pin");
        assert_eq!(val, Some("tent-area".to_string()));
    }
}
