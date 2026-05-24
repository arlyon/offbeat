use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::types::PeerInfo;

/// Source of peer discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerSource {
    Crdt,
    Ble,
    Gossip,
}

/// Gossip connection status for a peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipStatus {
    Unknown,
    Joining,
    Active,
    Stale,
}

/// Tracked state for a known peer.
#[derive(Debug, Clone)]
pub struct PeerEntry {
    pub endpoint_id: String,
    pub relay_url: Option<String>,
    pub last_seen: u64,
    pub source: PeerSource,
    pub ble_prefix_match: bool,
    pub gossip_status: GossipStatus,
    pub last_join_attempt: Option<Instant>,
}

/// Manages multi-path peer discovery and connection bootstrapping.
///
/// Tracks peers discovered from CRDT peer lists, BLE scans, and gossip
/// neighbor events. The actual tick loops (peer discovery, heartbeat,
/// reconnect) are wired up at the OffbeatNode level, since they require
/// async access to the endpoint, gossip, and BLE transport.
pub struct ConnectionManager {
    peers: Arc<Mutex<HashMap<String, PeerEntry>>>,
    own_endpoint_id: String,
}

impl ConnectionManager {
    pub fn new(own_endpoint_id: String) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            own_endpoint_id,
        }
    }

    /// Update the peer table from a CRDT peer list.
    ///
    /// Inserts new peers and updates existing ones with fresh relay URLs and
    /// timestamps. Peers whose `endpoint_id` matches `own_endpoint_id` are
    /// filtered out. Existing peers not present in the new list are retained
    /// (they may have been discovered via a different source).
    pub fn on_peer_list_updated(&self, peers: Vec<PeerInfo>) {
        let mut table = self.peers.lock().expect("peer table lock poisoned");
        for peer in peers {
            if peer.endpoint_id == self.own_endpoint_id {
                continue;
            }
            match table.get_mut(&peer.endpoint_id) {
                Some(entry) => {
                    // Merge: update relay_url and last_seen from the CRDT,
                    // but preserve gossip_status and ble_prefix_match since
                    // those come from other sources.
                    entry.relay_url = peer.relay_url;
                    entry.last_seen = peer.last_seen;
                }
                None => {
                    table.insert(
                        peer.endpoint_id.clone(),
                        PeerEntry {
                            endpoint_id: peer.endpoint_id,
                            relay_url: peer.relay_url,
                            last_seen: peer.last_seen,
                            source: PeerSource::Crdt,
                            ble_prefix_match: false,
                            gossip_status: GossipStatus::Unknown,
                            last_join_attempt: None,
                        },
                    );
                }
            }
        }
    }

    /// Get a snapshot of all known peers.
    pub fn peer_snapshot(&self) -> Vec<PeerEntry> {
        let table = self.peers.lock().expect("peer table lock poisoned");
        table.values().cloned().collect()
    }

    /// Get the number of active peers (those with `GossipStatus::Active`).
    pub fn active_peer_count(&self) -> u32 {
        let table = self.peers.lock().expect("peer table lock poisoned");
        table
            .values()
            .filter(|e| e.gossip_status == GossipStatus::Active)
            .count() as u32
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_peer(endpoint_id: &str, relay_url: Option<&str>, last_seen: u64) -> PeerInfo {
        PeerInfo {
            endpoint_id: endpoint_id.to_string(),
            relay_url: relay_url.map(|s| s.to_string()),
            last_seen,
            user_id: format!("user-{endpoint_id}"),
        }
    }

    #[test]
    fn test_on_peer_list_updated() {
        let cm = ConnectionManager::new("own-id".to_string());

        let peers = vec![
            make_peer("peer-a", Some("https://relay.example.com"), 1700000000),
            make_peer("peer-b", None, 1700001000),
        ];
        cm.on_peer_list_updated(peers);

        let snapshot = cm.peer_snapshot();
        assert_eq!(snapshot.len(), 2);

        let a = snapshot.iter().find(|e| e.endpoint_id == "peer-a").unwrap();
        assert_eq!(a.relay_url.as_deref(), Some("https://relay.example.com"));
        assert_eq!(a.last_seen, 1700000000);
        assert_eq!(a.source, PeerSource::Crdt);
        assert_eq!(a.gossip_status, GossipStatus::Unknown);
        assert!(!a.ble_prefix_match);

        let b = snapshot.iter().find(|e| e.endpoint_id == "peer-b").unwrap();
        assert!(b.relay_url.is_none());
        assert_eq!(b.last_seen, 1700001000);
    }

    #[test]
    fn test_filters_own_endpoint() {
        let own_id = "my-endpoint-id";
        let cm = ConnectionManager::new(own_id.to_string());

        let peers = vec![
            make_peer(own_id, None, 1700000000),
            make_peer("other-peer", None, 1700001000),
        ];
        cm.on_peer_list_updated(peers);

        let snapshot = cm.peer_snapshot();
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].endpoint_id, "other-peer");
    }

    #[test]
    fn test_active_peer_count() {
        let cm = ConnectionManager::new("own-id".to_string());

        let peers = vec![
            make_peer("peer-a", None, 1700000000),
            make_peer("peer-b", None, 1700001000),
            make_peer("peer-c", None, 1700002000),
        ];
        cm.on_peer_list_updated(peers);

        // All peers start with GossipStatus::Unknown, so active count is 0.
        assert_eq!(cm.active_peer_count(), 0);

        // Manually set two peers to Active.
        {
            let mut table = cm.peers.lock().unwrap();
            table.get_mut("peer-a").unwrap().gossip_status = GossipStatus::Active;
            table.get_mut("peer-c").unwrap().gossip_status = GossipStatus::Active;
        }

        assert_eq!(cm.active_peer_count(), 2);
    }

    #[test]
    fn test_peer_list_update_merges() {
        let cm = ConnectionManager::new("own-id".to_string());

        // First update: two peers.
        let peers_v1 = vec![
            make_peer("peer-a", Some("https://relay-1.example.com"), 1700000000),
            make_peer("peer-b", None, 1700001000),
        ];
        cm.on_peer_list_updated(peers_v1);
        assert_eq!(cm.peer_snapshot().len(), 2);

        // Mark peer-a as Active before the merge.
        {
            let mut table = cm.peers.lock().unwrap();
            table.get_mut("peer-a").unwrap().gossip_status = GossipStatus::Active;
            table.get_mut("peer-a").unwrap().ble_prefix_match = true;
        }

        // Second update: peer-a updated, peer-c new, peer-b not mentioned.
        let peers_v2 = vec![
            make_peer("peer-a", Some("https://relay-2.example.com"), 1700005000),
            make_peer("peer-c", None, 1700006000),
        ];
        cm.on_peer_list_updated(peers_v2);

        let snapshot = cm.peer_snapshot();
        // All three peers should be present (peer-b retained from v1).
        assert_eq!(snapshot.len(), 3);

        // peer-a: relay_url and last_seen updated, but gossip_status and
        // ble_prefix_match preserved from before the merge.
        let a = snapshot.iter().find(|e| e.endpoint_id == "peer-a").unwrap();
        assert_eq!(a.relay_url.as_deref(), Some("https://relay-2.example.com"));
        assert_eq!(a.last_seen, 1700005000);
        assert_eq!(a.gossip_status, GossipStatus::Active);
        assert!(a.ble_prefix_match);

        // peer-b: unchanged from v1.
        let b = snapshot.iter().find(|e| e.endpoint_id == "peer-b").unwrap();
        assert!(b.relay_url.is_none());
        assert_eq!(b.last_seen, 1700001000);

        // peer-c: newly added.
        let c = snapshot.iter().find(|e| e.endpoint_id == "peer-c").unwrap();
        assert!(c.relay_url.is_none());
        assert_eq!(c.last_seen, 1700006000);
        assert_eq!(c.source, PeerSource::Crdt);
    }
}
