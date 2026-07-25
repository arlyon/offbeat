//! P2P group discovery handshake protocol.
//!
//! When two iroh peers connect, they run a zero-knowledge set intersection
//! to discover shared groups without leaking membership information.
//!
//! Protocol:
//! 1. Peer A generates 16-byte random nonce
//! 2. For each group key, computes `token = blake3(key || "hs" || nonce)`
//! 3. Sends `GroupHandshake { session_nonce, tokens[] }`
//! 4. Peer B computes same tokens for own groups, finds intersection
//! 5. For each shared group, B sends `GroupSyncOffer { group_id, encrypted_sv }`
//! 6. A decrypts SVs, computes Yrs diffs, sends encrypted diffs back

use crate::crypto;
use crate::doc_manager::DocManager;
use crate::proto;

/// Build a handshake message from local group keys.
///
/// Generates a random 16-byte nonce and computes a blake3 token per group.
pub fn build_handshake(group_keys: &[[u8; 32]]) -> proto::GroupHandshake {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).expect("getrandom failed");
    build_handshake_with_nonce(group_keys, &nonce)
}

/// Build a handshake with a specific nonce (for deterministic testing).
pub fn build_handshake_with_nonce(
    group_keys: &[[u8; 32]],
    nonce: &[u8; 16],
) -> proto::GroupHandshake {
    let tokens = group_keys
        .iter()
        .map(|key| compute_handshake_token(key, nonce).to_vec())
        .collect();
    proto::GroupHandshake {
        session_nonce: nonce.to_vec(),
        tokens,
    }
}

/// Compute a single handshake token: `blake3(key || "hs" || nonce)`.
fn compute_handshake_token(key: &[u8; 32], nonce: &[u8]) -> [u8; 32] {
    let mut input = Vec::with_capacity(32 + 2 + nonce.len());
    input.extend_from_slice(key);
    input.extend_from_slice(b"hs");
    input.extend_from_slice(nonce);
    *blake3::hash(&input).as_bytes()
}

/// Given local groups and a remote handshake, find groups we share.
///
/// Returns `(group_id, group_key)` pairs for shared groups.
pub fn find_shared_groups(
    local_groups: &[(String, [u8; 32])],
    remote: &proto::GroupHandshake,
) -> Vec<(String, [u8; 32])> {
    let nonce: &[u8] = &remote.session_nonce;
    let mut shared = Vec::new();

    for (group_id, key) in local_groups {
        let local_token = compute_handshake_token(key, nonce);
        if remote
            .tokens
            .iter()
            .any(|t| t.as_slice() == local_token.as_slice())
        {
            shared.push((group_id.clone(), *key));
        }
    }

    shared
}

/// Build sync offers for shared groups.
///
/// Each offer contains the group's encrypted state vector so the remote
/// peer can compute a targeted diff.
pub fn build_sync_offers(
    shared: &[(String, [u8; 32])],
    doc_manager: &DocManager,
) -> anyhow::Result<Vec<proto::GroupSyncOffer>> {
    let mut offers = Vec::with_capacity(shared.len());

    for (group_id, key) in shared {
        let doc_id = format!("group/{group_id}/state");
        doc_manager.get_or_create(&doc_id);
        let sv = doc_manager.get_state_vector(&doc_id)?;
        let encrypted_sv = crypto::encrypt(key, &sv)?;
        let key_id = crypto::group_id_from_key(key);

        offers.push(proto::GroupSyncOffer {
            group_id: group_id.clone(),
            encrypted_sv,
            group_key_id: key_id,
        });
    }

    Ok(offers)
}

/// Process received sync offers: decrypt SVs, compute diffs, return encrypted diffs.
///
/// Returns `(group_id, encrypted_diff)` pairs to send back to the offering peer.
pub fn apply_sync_offers(
    offers: &[proto::GroupSyncOffer],
    local_groups: &[(String, [u8; 32])],
    doc_manager: &DocManager,
) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    let mut diffs = Vec::new();

    for offer in offers {
        // Find the key for this group
        let key = local_groups
            .iter()
            .find(|(id, _)| *id == offer.group_id)
            .map(|(_, k)| k)
            .ok_or_else(|| anyhow::anyhow!("no key for group {}", offer.group_id))?;

        let remote_sv = crypto::decrypt(key, &offer.encrypted_sv)?;
        let doc_id = format!("group/{}/state", offer.group_id);
        doc_manager.get_or_create(&doc_id);
        let diff = doc_manager.encode_diff(&doc_id, &remote_sv)?;
        let encrypted_diff = crypto::encrypt(key, &diff)?;

        diffs.push((offer.group_id.clone(), encrypted_diff));
    }

    Ok(diffs)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::groups::GroupManager;
    use prost::Message;
    use std::sync::Arc;

    fn test_key() -> [u8; 32] {
        crypto::generate_group_key()
    }

    // -----------------------------------------------------------------------
    // build_handshake + find_shared_groups roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_roundtrip_2_shared_out_of_5() {
        let shared_keys = [test_key(), test_key()];
        let only_a_keys = [test_key(), test_key()];
        let only_b_keys = [test_key()];

        // Peer A has shared[0..2] + only_a[0..2]
        let mut a_keys: Vec<[u8; 32]> = shared_keys.to_vec();
        a_keys.extend_from_slice(&only_a_keys);

        // Peer B has shared[0..2] + only_b[0]
        let mut b_groups: Vec<(String, [u8; 32])> = shared_keys
            .iter()
            .map(|k| (crypto::group_id_from_key(k), *k))
            .collect();
        b_groups.push((crypto::group_id_from_key(&only_b_keys[0]), only_b_keys[0]));

        // A builds handshake
        let hs = build_handshake(&a_keys);
        assert_eq!(hs.tokens.len(), 4);
        assert_eq!(hs.session_nonce.len(), 16);

        // B finds shared groups
        let shared = find_shared_groups(&b_groups, &hs);
        assert_eq!(shared.len(), 2, "should find exactly 2 shared groups");

        let shared_ids: Vec<&str> = shared.iter().map(|(id, _)| id.as_str()).collect();
        assert!(shared_ids.contains(&crypto::group_id_from_key(&shared_keys[0]).as_str()));
        assert!(shared_ids.contains(&crypto::group_id_from_key(&shared_keys[1]).as_str()));
    }

    #[test]
    fn test_no_shared_groups() {
        let a_keys = [test_key(), test_key()];
        let b_groups: Vec<(String, [u8; 32])> =
            vec![(crypto::group_id_from_key(&test_key()), test_key())];

        let hs = build_handshake(&a_keys);
        let shared = find_shared_groups(&b_groups, &hs);
        assert!(shared.is_empty());
    }

    #[test]
    fn test_deterministic_same_nonce_same_keys() {
        let keys = [test_key(), test_key()];
        let nonce = [42u8; 16];

        let hs1 = build_handshake_with_nonce(&keys, &nonce);
        let hs2 = build_handshake_with_nonce(&keys, &nonce);

        assert_eq!(hs1.tokens, hs2.tokens);
    }

    #[test]
    fn test_session_scoped_different_nonce() {
        let keys = [test_key()];
        let nonce1 = [1u8; 16];
        let nonce2 = [2u8; 16];

        let hs1 = build_handshake_with_nonce(&keys, &nonce1);
        let hs2 = build_handshake_with_nonce(&keys, &nonce2);

        assert_ne!(hs1.tokens, hs2.tokens);
    }

    // -----------------------------------------------------------------------
    // Sync offers
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_sync_offers() {
        let db = Arc::new(Database::new_in_memory().unwrap());
        let dm = DocManager::new(db.clone());
        let key = test_key();
        let group_id = crypto::group_id_from_key(&key);

        // Create a doc with some data
        let doc_id = format!("group/{group_id}/state");
        dm.set_map_value(&doc_id, "name", "Test").unwrap();

        let shared = vec![(group_id.clone(), key)];
        let offers = build_sync_offers(&shared, &dm).unwrap();

        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].group_id, group_id);
        assert_eq!(offers[0].group_key_id, group_id);
        assert!(!offers[0].encrypted_sv.is_empty());

        // Verify we can decrypt the SV
        let sv = crypto::decrypt(&key, &offers[0].encrypted_sv).unwrap();
        assert!(!sv.is_empty());
    }

    #[test]
    fn test_apply_sync_offers() {
        let db_a = Arc::new(Database::new_in_memory().unwrap());
        let dm_a = DocManager::new(db_a.clone());
        let db_b = Arc::new(Database::new_in_memory().unwrap());
        let dm_b = DocManager::new(db_b.clone());

        let key = test_key();
        let group_id = crypto::group_id_from_key(&key);
        let doc_id = format!("group/{group_id}/state");

        // Peer A has data
        dm_a.set_map_value(&doc_id, "name", "Test Group").unwrap();
        dm_a.set_map_value(&doc_id, "member/alice", r#"{"displayName":"Alice"}"#)
            .unwrap();

        // Peer B has less data
        dm_b.set_map_value(&doc_id, "name", "Test Group").unwrap();

        let shared = vec![(group_id.clone(), key)];

        // B builds offers (B's SV)
        let offers = build_sync_offers(&shared, &dm_b).unwrap();

        // A processes B's offers (computes diff from B's SV to A's state)
        let diffs = apply_sync_offers(&offers, &shared, &dm_a).unwrap();

        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].0, group_id);

        // B applies A's diff
        let plaintext_diff = crypto::decrypt(&key, &diffs[0].1).unwrap();
        dm_b.apply_update(&doc_id, &plaintext_diff).unwrap();

        // B should now see Alice
        let member = dm_b.read_map_value(&doc_id, "member/alice");
        assert!(member.is_some(), "B should see Alice after applying diff");
    }

    // -----------------------------------------------------------------------
    // Full bilateral sync
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_full_bilateral_sync() {
        let db_a = Arc::new(Database::new_in_memory().unwrap());
        let dm_a = Arc::new(DocManager::new(db_a.clone()));
        let gm_a = GroupManager::new(db_a.clone(), dm_a.clone());

        let db_b = Arc::new(Database::new_in_memory().unwrap());
        let dm_b = Arc::new(DocManager::new(db_b.clone()));
        let gm_b = GroupManager::new(db_b.clone(), dm_b.clone());

        // A creates group and adds data
        let create = gm_a
            .create_group("fest-1", "Crew", "alice", "Alice")
            .await
            .unwrap();
        let group_id = &create.group_id;

        gm_a.check_in(group_id, "alice", Some("main-stage"), None)
            .await
            .unwrap();
        gm_a.add_pin(group_id, "pin-1", "Meeting spot", "0,0", "alice")
            .await
            .unwrap();

        // B joins the group
        gm_b.join_group(&create.invite_payload, "bob", "Bob")
            .await
            .unwrap();

        let key_a = db_a.load_group_key(group_id).unwrap().unwrap();
        let key_b = db_b.load_group_key(group_id).unwrap().unwrap();
        assert_eq!(key_a, key_b);

        // Run handshake: A → B
        let a_keys = vec![key_a];
        let b_groups = vec![(group_id.clone(), key_b)];

        let hs = build_handshake(&a_keys);
        let shared = find_shared_groups(&b_groups, &hs);
        assert_eq!(shared.len(), 1);

        // B builds offers (sends B's SV to A)
        let offers = build_sync_offers(&shared, &dm_b).unwrap();

        // A computes diffs from B's SV
        let a_groups = vec![(group_id.clone(), key_a)];
        let diffs_a_to_b = apply_sync_offers(&offers, &a_groups, &dm_a).unwrap();

        // B applies A's diffs
        let doc_id = format!("group/{group_id}/state");
        for (_, encrypted_diff) in &diffs_a_to_b {
            let diff = crypto::decrypt(&key_b, encrypted_diff).unwrap();
            dm_b.apply_update(&doc_id, &diff).unwrap();
        }

        // Now do reverse: B → A (send B's changes to A)
        let offers_a = build_sync_offers(&a_groups, &dm_a).unwrap();
        let diffs_b_to_a = apply_sync_offers(&offers_a, &shared, &dm_b).unwrap();
        for (_, encrypted_diff) in &diffs_b_to_a {
            let diff = crypto::decrypt(&key_a, encrypted_diff).unwrap();
            dm_a.apply_update(&doc_id, &diff).unwrap();
        }

        // Both should now have identical state
        let state_a = gm_a.get_group_state(group_id).await.unwrap();
        let state_b = gm_b.get_group_state(group_id).await.unwrap();

        assert_eq!(state_a.name, state_b.name);
        assert_eq!(state_a.members.len(), state_b.members.len());
        assert_eq!(state_a.pins.len(), state_b.pins.len());
        assert_eq!(state_a.pins[0].label, "Meeting spot");

        // A should see bob
        let bob_a = state_a.members.iter().find(|m| m.user_id == "bob");
        assert!(bob_a.is_some(), "A should see Bob after bilateral sync");

        // B should see alice's location
        let alice_b = state_b.members.iter().find(|m| m.user_id == "alice");
        assert!(alice_b.is_some(), "B should see Alice after bilateral sync");
        assert_eq!(alice_b.unwrap().stage_id.as_deref(), Some("main-stage"));
    }

    // -----------------------------------------------------------------------
    // Protobuf roundtrip
    // -----------------------------------------------------------------------

    #[test]
    fn test_protobuf_roundtrip_handshake() {
        let keys = [test_key(), test_key()];
        let hs = build_handshake(&keys);

        let bytes = hs.encode_to_vec();
        let decoded = proto::GroupHandshake::decode(bytes.as_slice()).unwrap();

        assert_eq!(decoded.session_nonce, hs.session_nonce);
        assert_eq!(decoded.tokens.len(), 2);
        assert_eq!(decoded.tokens, hs.tokens);
    }

    #[test]
    fn test_protobuf_roundtrip_response() {
        let response = proto::GroupHandshakeResponse {
            offers: vec![proto::GroupSyncOffer {
                group_id: "group-abc".to_string(),
                encrypted_sv: vec![1, 2, 3],
                group_key_id: "key-id".to_string(),
            }],
        };

        let bytes = response.encode_to_vec();
        let decoded = proto::GroupHandshakeResponse::decode(bytes.as_slice()).unwrap();

        assert_eq!(decoded.offers.len(), 1);
        assert_eq!(decoded.offers[0].group_id, "group-abc");
        assert_eq!(decoded.offers[0].encrypted_sv, vec![1, 2, 3]);
    }

    #[test]
    fn test_protobuf_roundtrip_sync_offer() {
        let offer = proto::GroupSyncOffer {
            group_id: "g1".to_string(),
            encrypted_sv: vec![10, 20, 30],
            group_key_id: "kid".to_string(),
        };

        let bytes = offer.encode_to_vec();
        let decoded = proto::GroupSyncOffer::decode(bytes.as_slice()).unwrap();

        assert_eq!(decoded.group_id, "g1");
        assert_eq!(decoded.encrypted_sv, vec![10, 20, 30]);
        assert_eq!(decoded.group_key_id, "kid");
    }
}
