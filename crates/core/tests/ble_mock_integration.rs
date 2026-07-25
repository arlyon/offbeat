use base64::Engine;
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;
use tokio::time::timeout;

use iroh_ble_transport::transport::routing::prefix_from_endpoint;
use iroh_ble_transport::transport::test_util::MockFabric;
use iroh_ble_transport::{BleTransport, DeviceId, InMemoryPeerStore, L2capPolicy};
use offbeat_core::OffbeatNode;
use offbeat_core::db::Database;
use offbeat_core::resource::Resource;

#[tokio::test]
async fn test_ble_discovery_and_sync() {
    let _ = tracing_subscriber::fmt::try_init();

    let fabric = Arc::new(MockFabric::new());

    // Setup Node A (Alice)
    let dir_a = tempdir().unwrap();
    let db_path_a = dir_a.path().join("alice.db");
    let db_a = Arc::new(Database::new(&db_path_a).unwrap());
    let secret_a = iroh::SecretKey::generate();
    db_a.save_iroh_secret_key(&secret_a).unwrap();
    let ep_a = secret_a.public();
    let device_a = DeviceId::from("alice");

    // Setup Node B (Bob)
    let dir_b = tempdir().unwrap();
    let db_path_b = dir_b.path().join("bob.db");
    let db_b = Arc::new(Database::new(&db_path_b).unwrap());
    let secret_b = iroh::SecretKey::generate();
    db_b.save_iroh_secret_key(&secret_b).unwrap();
    let ep_b = secret_b.public();
    let device_b = DeviceId::from("bob");

    // Create Alice's transport
    let (inbox_tx_a, _inbox_rx_a) = tokio::sync::mpsc::channel(16);
    let iface_a = fabric.add_node(device_a.clone(), inbox_tx_a);
    // In this test setup, Alice's dongle acts as the client.
    // When Alice (client) reads from Bob (device_b), her dongle needs to return Bob's ID.
    iface_a.set_endpoint_id_default(Some(ep_b.as_bytes().to_vec()));
    iface_a.set_version_default(Some(1));

    let ble_a = BleTransport::new_for_test(
        ep_a,
        iface_a,
        Arc::new(InMemoryPeerStore::new()),
        L2capPolicy::Disabled,
    )
    .await;
    fabric.add_node(device_a.clone(), ble_a.handle().inbox.clone());

    let node_a = OffbeatNode::new_with_networking_and_transport(db_a.clone(), Some(ble_a))
        .await
        .unwrap();

    // Create Bob's transport
    let (inbox_tx_b, _inbox_rx_b) = tokio::sync::mpsc::channel(16);
    let iface_b = fabric.add_node(device_b.clone(), inbox_tx_b);
    // When Bob (client) reads from Alice (device_a), his dongle needs to return Alice's ID.
    iface_b.set_endpoint_id_default(Some(ep_a.as_bytes().to_vec()));
    iface_b.set_version_default(Some(1));

    let ble_b = BleTransport::new_for_test(
        ep_b,
        iface_b,
        Arc::new(InMemoryPeerStore::new()),
        L2capPolicy::Disabled,
    )
    .await;
    fabric.add_node(device_b.clone(), ble_b.handle().inbox.clone());

    let node_b = OffbeatNode::new_with_networking_and_transport(db_b.clone(), Some(ble_b))
        .await
        .unwrap();

    // Start BLE connection tasks for both nodes
    let _handles_a = node_a.spawn_ble_sync();
    let _handles_b = node_b.spawn_ble_sync();

    // Alice creates a group
    let festival_id = "fest";
    let res = node_a
        .group_manager
        .create_group(festival_id, "My Group", "alice-user", "Alice")
        .await
        .unwrap();
    let group_id = res.group_id;
    let group_key = db_a.load_group_key(&group_id).unwrap().unwrap();

    // Construct invite URL
    let b64key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(group_key);
    let invite_url = format!("offbeat://group/{}/{}/{}", festival_id, group_id, b64key);

    // Alice creates a document in that group
    let doc_id = format!("group/{}/state", group_id);
    let _doc_a = node_a.doc_manager.get_or_create(&doc_id);
    node_a
        .doc_manager
        .set_map_value(&doc_id, "hello", "world")
        .unwrap();

    // Register it so it's syncable
    let resource = Resource::group_state(group_key);
    node_a
        .resource_registry
        .write()
        .unwrap()
        .register(resource.clone());

    // Bob joins the group
    node_b
        .group_manager
        .join_group(&invite_url, "bob-user", "Bob")
        .await
        .unwrap();
    // Bob also interests in the document
    node_b
        .resource_registry
        .write()
        .unwrap()
        .register(resource.clone());

    // Simulate discovery
    fabric.advertise(
        device_b.clone(),
        device_a.clone(),
        prefix_from_endpoint(&ep_b),
    );
    fabric.advertise(
        device_a.clone(),
        device_b.clone(),
        prefix_from_endpoint(&ep_a),
    );

    // Wait for sync
    timeout(Duration::from_secs(15), async {
        loop {
            if let Some(val) = node_b.doc_manager.read_map_value(&doc_id, "hello")
                && val == "world"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Bob should have received the synced value");
}
