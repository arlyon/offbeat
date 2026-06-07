use std::sync::Arc;
use offbeat_core::gossip_manager::{GossipManager, GossipMessage};
use offbeat_core::db::Database;
use offbeat_core::doc_manager::DocManager;
use offbeat_core::crypto;
use iroh_gossip::net::Gossip;
use iroh::endpoint::presets;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_gossip_manager_dispatch_edge_cases() {
    let db = Arc::new(Database::new_in_memory().unwrap());
    let dm = Arc::new(DocManager::new(db.clone()));
    
    // Setup a real-ish Gossip instance but on a local endpoint
    let ep = iroh::Endpoint::builder(presets::N0).bind().await.unwrap();
    let gossip = Gossip::builder().spawn(ep);
    let gm = Arc::new(Mutex::new(GossipManager::new(gossip)));
    
    let group_key = [7u8; 32];
    let group_id = crypto::group_id_from_key(&group_key);
    db.save_group(&group_id, "fest", "Group", &group_key).unwrap();

    // Edge Case 1: Malformed encrypted update
    let result = offbeat_core::gossip_manager::dispatch_message(
        &dm,
        &db,
        GossipMessage::GroupUpdate {
            doc_id: "group/test/state".to_string(),
            encrypted: vec![1, 2, 3], // Garbage
            group_key,
        },
        &[0u8; 32],
    );
    assert!(result.is_err(), "Malformed encrypted update should fail");

    // Edge Case 2: Decryption with wrong key (simulated by garbage key)
    let wrong_key = [1u8; 32];
    let result = offbeat_core::gossip_manager::dispatch_message(
        &dm,
        &db,
        GossipMessage::GroupUpdate {
            doc_id: "group/test/state".to_string(),
            encrypted: vec![0; 32], // Valid-ish length but wrong content
            group_key: wrong_key,
        },
        &[0u8; 32],
    );
    assert!(result.is_err(), "Decryption with wrong key should fail");

    // Edge Case 3: Festival update with invalid signature
    let result = offbeat_core::gossip_manager::dispatch_message(
        &dm,
        &db,
        GossipMessage::FestivalUpdate {
            doc_id: "fest/state".to_string(),
            signed_update: offbeat_core::types::SignedUpdate {
                update: vec![],
                author: "attacker".to_string(),
                signature: vec![0; 64],
            },
        },
        &[2u8; 32], // Some public key
    );
    assert!(result.is_err(), "Invalid signature should be rejected");
}

#[tokio::test]
async fn test_sync_orchestrator_full_chain_malformed_protobuf() {
    let db = Arc::new(Database::new_in_memory().unwrap());
    let dm = Arc::new(DocManager::new(db.clone()));
    let cm = Arc::new(offbeat_core::chat::ChatManager::new(db.clone(), dm.clone()));
    let reg = Arc::new(std::sync::RwLock::new(offbeat_core::resource::ResourceRegistry::new()));
    let notifier = offbeat_core::notifier::ResourceNotifier::new_arc();
    let orch = offbeat_core::sync::SyncOrchestrator::new(reg, dm, cm, db, notifier);

    // Feed totally random bytes into the Protobuf decoder
    let result = orch.handle_incoming_bytes("some/topic", b"not a protobuf").await;
    assert!(result.is_err(), "Garbage bytes should fail Protobuf decoding");
}

#[tokio::test]
async fn test_connection_manager_mac_rotation_merge() {
    let own_id = "my-id";
    let cm = offbeat_core::connection_manager::ConnectionManager::new(own_id.to_string());
    
    let peer_id = "alice-pk";
    let endpoint_str = peer_id.to_string();

    // 1. Peer discovered via BLE
    cm.on_ble_peer_discovered(&endpoint_str);
    let entry = cm.peer_snapshot().into_iter().find(|p| p.endpoint_id == endpoint_str).unwrap();
    assert_eq!(entry.source, offbeat_core::connection_manager::PeerSource::Ble);

    // 2. Peer rotated MAC (simulated by discovery again, entry should persist)
    cm.on_ble_peer_discovered(&endpoint_str);
    let entry2 = cm.peer_snapshot().into_iter().find(|p| p.endpoint_id == endpoint_str).unwrap();
    assert_eq!(entry2.endpoint_id, endpoint_str);
    assert_eq!(entry2.source, offbeat_core::connection_manager::PeerSource::Ble);
}

#[tokio::test]
async fn test_connection_manager_join_storm_throttle() {
    let cm = offbeat_core::connection_manager::ConnectionManager::new("me".to_string());
    let peer = "bob";
    let interval = std::time::Duration::from_secs(60);

    // Initial check: should nudge (peer is unknown)
    assert!(cm.should_nudge_join(peer, interval));

    // Peer discovered via BLE
    cm.on_ble_peer_discovered(peer);

    // After nudge: should NOT nudge again immediately
    cm.mark_join_attempted(peer);
    assert!(!cm.should_nudge_join(peer, interval));

    // Different peer: should nudge
    assert!(cm.should_nudge_join("charlie", interval));
}

#[tokio::test]
async fn test_database_bootstrap_peer_pruning() {
    let db = Database::new_in_memory().unwrap();
    let fest = "glastonbury";

    // Insert 10 peers
    for i in 0..10 {
        db.upsert_festival_peer(
            fest,
            &format!("peer-{i}"),
            None,
            1000 + i,
            "test"
        ).unwrap();
    }

    // Prune to keep only the 3 freshest
    db.prune_festival_peers(fest, 3).unwrap();
    let peers = db.load_festival_peers(fest, 10).unwrap();
    
    assert_eq!(peers.len(), 3);
    assert_eq!(peers[0].endpoint_id, "peer-9");
    assert_eq!(peers[2].endpoint_id, "peer-7");
}
