use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::db::{BootstrapPeer, Database};
use crate::types::PeerInfo;

/// Source of peer discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerSource {
    Crdt,
    Ble,
    Gossip,
}

impl PeerSource {
    /// Stable string tag persisted in the `festival_peers.source` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            PeerSource::Crdt => "crdt",
            PeerSource::Ble => "ble",
            PeerSource::Gossip => "gossip",
        }
    }
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
    /// Durable peer directory. `None` in tests/contexts without persistence;
    /// when present, peer sightings are written through for offline cold-start.
    db: Option<Arc<Database>>,
}

impl ConnectionManager {
    pub fn new(own_endpoint_id: String) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            own_endpoint_id,
            db: None,
        }
    }

    /// Construct with a durable directory backing store.
    pub fn new_with_db(own_endpoint_id: String, db: Arc<Database>) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            own_endpoint_id,
            db: Some(db),
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

    /// Persist peers learned for a festival into the durable directory so the
    /// mesh can cold-start offline next session. No-op without a backing store.
    /// Peers matching `own_endpoint_id` are skipped. Errors are logged, not
    /// propagated — a directory write failure must never break live sync.
    pub fn record_festival_peers(&self, festival_id: &str, peers: &[PeerInfo], source: PeerSource) {
        let Some(db) = &self.db else { return };
        for peer in peers {
            if peer.endpoint_id == self.own_endpoint_id {
                continue;
            }
            if let Err(e) = db.upsert_festival_peer(
                festival_id,
                &peer.endpoint_id,
                peer.relay_url.as_deref(),
                peer.last_seen,
                source.as_str(),
            ) {
                tracing::warn!(festival_id, peer = %peer.endpoint_id, ?e, "failed to persist festival peer");
            }
        }
    }

    /// Persist a gossip neighbor (from a `NeighborUp` event) into the durable
    /// directory so the mesh can re-bootstrap from it next session. Source is
    /// tagged `gossip`; the sighting timestamp is now. No-op without a backing
    /// store or for our own endpoint. Errors are logged, never propagated.
    pub fn record_gossip_neighbor(&self, festival_id: &str, endpoint_id: &str) {
        let Some(db) = &self.db else { return };
        if endpoint_id == self.own_endpoint_id {
            return;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Err(e) =
            db.upsert_festival_peer(festival_id, endpoint_id, None, now, PeerSource::Gossip.as_str())
        {
            tracing::warn!(festival_id, peer = %endpoint_id, ?e, "failed to harvest gossip neighbor");
        }
    }

    /// Load the freshest bootstrap peers for a festival from the durable
    /// directory, newest first, capped at `limit`. Returns empty without a
    /// backing store or on read error. Phase 2 maps these to gossip bootstrap.
    pub fn bootstrap_peers(&self, festival_id: &str, limit: usize) -> Vec<BootstrapPeer> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        match db.load_festival_peers(festival_id, limit) {
            Ok(peers) => peers,
            Err(e) => {
                tracing::warn!(festival_id, ?e, "failed to load bootstrap peers");
                Vec::new()
            }
        }
    }

    /// Get the number of active peers (those with `GossipStatus::Active`).
    pub fn active_peer_count(&self) -> u32 {
        let table = self.peers.lock().expect("peer table lock poisoned");
        table
            .values()
            .filter(|e| e.gossip_status == GossipStatus::Active)
            .count() as u32
    }

    /// Upsert a peer discovered via BLE.
    pub fn on_ble_peer_discovered(&self, endpoint_id: &str) {
        let mut table = self.peers.lock().expect("peer table lock poisoned");
        if endpoint_id == self.own_endpoint_id {
            return;
        }
        table
            .entry(endpoint_id.to_string())
            .and_modify(|entry| {
                // BLE discovery is a more recent sighting; update source if it
                // was previously only known from CRDT.
                if entry.source == PeerSource::Crdt {
                    entry.source = PeerSource::Ble;
                }
            })
            .or_insert_with(|| PeerEntry {
                endpoint_id: endpoint_id.to_string(),
                relay_url: None,
                last_seen: 0,
                source: PeerSource::Ble,
                ble_prefix_match: true,
                gossip_status: GossipStatus::Unknown,
                last_join_attempt: None,
            });
    }

    /// Update the gossip status for a peer.
    pub fn set_gossip_status(&self, endpoint_id: &str, status: GossipStatus) {
        let mut table = self.peers.lock().expect("peer table lock poisoned");
        if let Some(entry) = table.get_mut(endpoint_id) {
            entry.gossip_status = status;
        }
    }

    /// Record that a join attempt was made for this peer.
    pub fn mark_join_attempted(&self, endpoint_id: &str) {
        let mut table = self.peers.lock().expect("peer table lock poisoned");
        if let Some(entry) = table.get_mut(endpoint_id) {
            entry.last_join_attempt = Some(Instant::now());
        }
    }

    /// Return endpoint IDs of peers that need a gossip join nudge.
    ///
    /// A peer needs a join if its gossip status is not Active and its last
    /// join attempt is either None or older than `min_interval`.
    pub fn peers_needing_join(&self, min_interval: Duration) -> Vec<String> {
        let table = self.peers.lock().expect("peer table lock poisoned");
        table
            .values()
            .filter(|e| {
                e.gossip_status != GossipStatus::Active
                    && match e.last_join_attempt {
                        None => true,
                        Some(t) => t.elapsed() >= min_interval,
                    }
            })
            .map(|e| e.endpoint_id.clone())
            .collect()
    }

    /// Per-peer throttle: returns true if enough time has passed since the
    /// last join attempt for this peer.
    pub fn should_nudge_join(&self, endpoint_id: &str, min_interval: Duration) -> bool {
        let table = self.peers.lock().expect("peer table lock poisoned");
        match table.get(endpoint_id) {
            None => true,
            Some(entry) => match entry.last_join_attempt {
                None => true,
                Some(t) => t.elapsed() >= min_interval,
            },
        }
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
    fn test_record_and_bootstrap_peers_roundtrip() {
        let db = Arc::new(Database::new_in_memory().unwrap());
        let cm = ConnectionManager::new_with_db("own-id".to_string(), db);

        let peers = vec![
            make_peer("peer-a", Some("https://relay"), 100),
            make_peer("peer-b", None, 200),
            make_peer("own-id", None, 300), // must be skipped
        ];
        cm.record_festival_peers("fest-1", &peers, PeerSource::Crdt);

        let boot = cm.bootstrap_peers("fest-1", 10);
        assert_eq!(boot.len(), 2);
        // Newest first.
        assert_eq!(boot[0].endpoint_id, "peer-b");
        assert_eq!(boot[0].source, "crdt");
        assert_eq!(boot[1].endpoint_id, "peer-a");
        assert_eq!(boot[1].relay_url.as_deref(), Some("https://relay"));
    }

    #[test]
    fn test_bootstrap_peers_without_db_is_empty() {
        let cm = ConnectionManager::new("own-id".to_string());
        cm.record_festival_peers("fest-1", &[make_peer("peer-a", None, 100)], PeerSource::Crdt);
        assert!(cm.bootstrap_peers("fest-1", 10).is_empty());
    }

    #[test]
    fn test_record_gossip_neighbor_persists_to_directory() {
        let db = Arc::new(Database::new_in_memory().unwrap());
        let cm = ConnectionManager::new_with_db("own-id".to_string(), db);

        cm.record_gossip_neighbor("fest-1", "neighbor-x");
        cm.record_gossip_neighbor("fest-1", "own-id"); // skipped

        let boot = cm.bootstrap_peers("fest-1", 10);
        assert_eq!(boot.len(), 1);
        assert_eq!(boot[0].endpoint_id, "neighbor-x");
        assert_eq!(boot[0].source, "gossip");
        assert!(boot[0].last_seen > 0);
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
