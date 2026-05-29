//! BLE peer auto-connection tasks.
//!
//! The BLE transport is intentionally passive — it won't auto-retry dead peers.
//! Reconnect policy lives here in the app layer, analogous to the demo app's
//! `reconnect_tick` and `transport_state_tick`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use iroh::EndpointId;
use iroh_ble_transport::BleTransport;
use iroh_gossip::proto::TopicId;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::connection_manager::{ConnectionManager, GossipStatus};
use crate::doc_manager::DocManager;
use crate::gossip_manager::{GossipManager, GossipReceiver};
use crate::sync::SyncOrchestrator;
use crate::sync_protocol::IrohSyncPeer;

/// Minimum interval between join nudges for a single peer (discovery tick).
const DISCOVERY_NUDGE_MIN_INTERVAL: Duration = Duration::from_secs(5);

/// Interval for the BLE discovery tick.
const DISCOVERY_TICK_INTERVAL: Duration = Duration::from_secs(1);

/// Interval for the reconnect tick.
const RECONNECT_TICK_INTERVAL: Duration = Duration::from_secs(10);

/// Minimum time between reconnect attempts for the same peer.
const RECONNECT_MIN_INTERVAL: Duration = Duration::from_secs(10);

/// Spawn background tasks for BLE peer auto-connection.
///
/// Returns join handles for the spawned tasks so they can be aborted on shutdown.
pub fn spawn_ble_connection_tasks(
    ble_transport: Arc<BleTransport>,
    gossip_manager: Arc<Mutex<GossipManager>>,
    connection_manager: Arc<ConnectionManager>,
    sync_orchestrator: Arc<SyncOrchestrator>,
    endpoint: Option<iroh::Endpoint>,
    doc_manager: Arc<DocManager>,
) -> Vec<JoinHandle<()>> {
    vec![
        // Task A: BLE discovery tick — poll BLE peers and nudge gossip join
        tokio::spawn(ble_discovery_tick(
            ble_transport.clone(),
            gossip_manager.clone(),
            connection_manager.clone(),
        )),
        // Task B: BLE reconnect tick — periodically retry non-active peers
        tokio::spawn(ble_reconnect_tick(
            ble_transport,
            gossip_manager.clone(),
            connection_manager.clone(),
        )),
        // Task C: Gossip event pump — drain receivers, update connection state,
        // and fire transient sv_exchange catch-up on NeighborUp.
        tokio::spawn(gossip_event_pump(
            gossip_manager,
            connection_manager,
            sync_orchestrator,
            endpoint,
            doc_manager,
        )),
    ]
}

/// Poll `ble_transport.snapshot_peers()` every second.
///
/// For each peer with a verified endpoint that is new or has transitioned to a
/// connected state, nudge gossip join immediately (with per-peer throttle).
async fn ble_discovery_tick(
    ble_transport: Arc<BleTransport>,
    gossip_manager: Arc<Mutex<GossipManager>>,
    connection_manager: Arc<ConnectionManager>,
) {
    // Track which peers we've seen and their last known phase, to detect transitions.
    let mut known_phases: HashMap<String, iroh_ble_transport::BlePeerPhase> = HashMap::new();

    loop {
        tokio::time::sleep(DISCOVERY_TICK_INTERVAL).await;

        let snapshot = ble_transport.snapshot_peers();
        let mut nudge_targets: Vec<EndpointId> = Vec::new();

        for info in &snapshot {
            let Some(endpoint_id) = info.verified_endpoint else {
                continue;
            };

            let endpoint_str = endpoint_id.to_string();

            // Upsert into ConnectionManager
            connection_manager.on_ble_peer_discovered(&endpoint_str);

            let prev_phase = known_phases.get(&endpoint_str).copied();
            let now_connected_or_forming = matches!(
                info.phase,
                iroh_ble_transport::BlePeerPhase::Discovered
                    | iroh_ble_transport::BlePeerPhase::Connecting
                    | iroh_ble_transport::BlePeerPhase::Handshaking
                    | iroh_ble_transport::BlePeerPhase::Connected
            );

            let is_fresh_sighting = prev_phase.is_none() && now_connected_or_forming;
            let is_ble_restored = !matches!(
                prev_phase,
                Some(iroh_ble_transport::BlePeerPhase::Connected)
            ) && matches!(info.phase, iroh_ble_transport::BlePeerPhase::Connected);

            if (is_fresh_sighting || is_ble_restored)
                && connection_manager.should_nudge_join(&endpoint_str, DISCOVERY_NUDGE_MIN_INTERVAL)
            {
                connection_manager.mark_join_attempted(&endpoint_str);
                nudge_targets.push(endpoint_id);
            }

            known_phases.insert(endpoint_str, info.phase);
        }

        // Remove stale entries for devices no longer in snapshot
        let current_ids: std::collections::HashSet<String> = snapshot
            .iter()
            .filter_map(|i| i.verified_endpoint.map(|e| e.to_string()))
            .collect();
        known_phases.retain(|k, _| current_ids.contains(k));

        if !nudge_targets.is_empty() {
            tracing::debug!(
                count = nudge_targets.len(),
                "ble_discovery_tick: nudging join for newly discovered peers"
            );
            let gm = gossip_manager.lock().await;
            gm.join_peers_all(nudge_targets).await;
        }
    }
}

/// Periodically retry gossip join for peers that aren't active yet.
///
/// Only attempts peers that the BLE transport still has a scan hint for,
/// preventing dial storms for absent peers.
async fn ble_reconnect_tick(
    ble_transport: Arc<BleTransport>,
    gossip_manager: Arc<Mutex<GossipManager>>,
    connection_manager: Arc<ConnectionManager>,
) {
    loop {
        tokio::time::sleep(RECONNECT_TICK_INTERVAL).await;

        let candidates = connection_manager.peers_needing_join(RECONNECT_MIN_INTERVAL);
        if candidates.is_empty() {
            continue;
        }

        // Filter to only peers the BLE transport currently has visibility of
        let targets: Vec<EndpointId> = candidates
            .iter()
            .filter_map(|endpoint_str| {
                let endpoint_id: EndpointId = endpoint_str.parse().ok()?;
                if ble_transport.has_scan_hint_for_endpoint(&endpoint_id) {
                    Some(endpoint_id)
                } else {
                    None
                }
            })
            .collect();

        if targets.is_empty() {
            continue;
        }

        // Mark all attempted
        for endpoint_str in &candidates {
            connection_manager.mark_join_attempted(endpoint_str);
        }

        tracing::debug!(
            count = targets.len(),
            "ble_reconnect_tick: retrying join for stale peers"
        );
        let gm = gossip_manager.lock().await;
        gm.join_peers_all(targets).await;
    }
}

/// Drain all gossip receivers, dispatching events to the sync orchestrator
/// and updating connection state on NeighborUp/NeighborDown.
async fn gossip_event_pump(
    gossip_manager: Arc<Mutex<GossipManager>>,
    connection_manager: Arc<ConnectionManager>,
    sync_orchestrator: Arc<SyncOrchestrator>,
    endpoint: Option<iroh::Endpoint>,
    doc_manager: Arc<DocManager>,
) {
    // Wait briefly to let subscriptions get established before draining
    tokio::time::sleep(Duration::from_millis(500)).await;

    let receivers: HashMap<TopicId, GossipReceiver> = {
        let mut gm = gossip_manager.lock().await;
        gm.take_receivers()
    };

    if receivers.is_empty() {
        tracing::debug!("gossip_event_pump: no receivers to drain, will poll periodically");
    }

    // Spawn a task per receiver, resolving each topic's festival so NeighborUp
    // peers can be harvested into the festival-scoped directory.
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    {
        let gm = gossip_manager.lock().await;
        for (topic_id, receiver) in receivers {
            let festival_id = gm.festival_for_topic(&topic_id);
            handles.push(tokio::spawn(pump_single_receiver(
                receiver,
                festival_id,
                connection_manager.clone(),
                sync_orchestrator.clone(),
                endpoint.clone(),
                doc_manager.clone(),
            )));
        }
    }

    // Periodically check for new receivers (from subscriptions created later)
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;

        let mut gm = gossip_manager.lock().await;
        let new_receivers: HashMap<TopicId, GossipReceiver> = gm.take_receivers();
        for (topic_id, receiver) in new_receivers {
            let festival_id = gm.festival_for_topic(&topic_id);
            handles.push(tokio::spawn(pump_single_receiver(
                receiver,
                festival_id,
                connection_manager.clone(),
                sync_orchestrator.clone(),
                endpoint.clone(),
                doc_manager.clone(),
            )));
        }
    }
}

/// Pump a single gossip receiver, handling events. `festival_id` is the
/// festival this topic belongs to (when known), used to scope neighbor harvest.
/// When an `endpoint` is present, a `NeighborUp` also fires a transient
/// `offbeat/sync/1` sv_exchange to catch up CRDT state from the new neighbor.
async fn pump_single_receiver(
    mut receiver: GossipReceiver,
    festival_id: Option<String>,
    connection_manager: Arc<ConnectionManager>,
    sync_orchestrator: Arc<SyncOrchestrator>,
    endpoint: Option<iroh::Endpoint>,
    doc_manager: Arc<DocManager>,
) {
    use iroh_gossip::api::Event;

    while let Some(event) = receiver.next().await {
        let event = match event {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!("gossip receiver error: {e}");
                continue;
            }
        };

        match event {
            Event::NeighborUp(peer_id) => {
                tracing::info!(peer = %peer_id.fmt_short(), "gossip NeighborUp");
                let peer_str = peer_id.to_string();
                connection_manager.set_gossip_status(&peer_str, GossipStatus::Active);
                // Neighbor harvest: persist into the durable directory so the
                // mesh can re-bootstrap from this peer offline next session.
                if let Some(fid) = &festival_id {
                    connection_manager.record_gossip_neighbor(fid, &peer_str);
                }
                // Reactive anti-entropy: open a transient offbeat/sync/1 channel
                // to the new neighbor and reconcile CRDT state. Spawned so it
                // doesn't block the event loop; failures are non-fatal (gossip
                // still carries live updates).
                if let Some(ep) = &endpoint {
                    let peer = IrohSyncPeer::new(ep.clone(), peer_id, doc_manager.clone());
                    let so = sync_orchestrator.clone();
                    tokio::spawn(async move {
                        if let Err(e) = so.sync_with_peer(&peer).await {
                            tracing::debug!("neighbor sv_exchange failed: {e}");
                        }
                    });
                }
            }
            Event::NeighborDown(peer_id) => {
                tracing::info!(peer = %peer_id.fmt_short(), "gossip NeighborDown");
                connection_manager.set_gossip_status(
                    &peer_id.to_string(),
                    GossipStatus::Stale,
                );
            }
            Event::Received(msg) => {
                // Dispatch through sync orchestrator
                // The topic isn't directly available from the Event, so we
                // dispatch as raw bytes and let the orchestrator decode.
                if let Err(e) = sync_orchestrator
                    .handle_incoming_bytes("gossip", &msg.content)
                    .await
                {
                    tracing::debug!("gossip message dispatch error: {e}");
                }
            }
            Event::Lagged => {
                tracing::warn!("gossip receiver lagged — some messages were dropped");
            }
        }
    }
}
