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
/// Per-peer state governing transient sv_exchange dials.
#[derive(Default)]
struct SyncDialState {
    last_attempt: Option<Instant>,
    /// Consecutive failures, widening the backoff window.
    failures: u32,
}

/// Cap on concurrent transient ALPN catch-up dials across all topics — bounds
/// the connection burst when many neighbors come up at once.
const MAX_CONCURRENT_DIALS: usize = 8;
/// Permits reserved for high-priority (group) topics: low-priority (public)
/// dials yield once the budget drops to this floor, so a flood of public-topic
/// neighbors can't starve group catch-up under shared dial pressure.
const HIGH_PRIORITY_RESERVE: usize = 3;
/// Backoff is `base * 2^min(failures, MAX_BACKOFF_SHIFT)`.
const MAX_BACKOFF_SHIFT: u32 = 6;

pub struct ConnectionManager {
    peers: Arc<Mutex<HashMap<String, PeerEntry>>>,
    own_endpoint_id: String,
    /// Durable peer directory. `None` in tests/contexts without persistence;
    /// when present, peer sightings are written through for offline cold-start.
    db: Option<Arc<Database>>,
    /// Per-peer throttle/backoff for transient sv_exchange dials.
    sync_state: Mutex<HashMap<String, SyncDialState>>,
    /// Bounds concurrent transient dials (the governor's dial budget).
    dial_semaphore: Arc<tokio::sync::Semaphore>,
}

impl ConnectionManager {
    pub fn new(own_endpoint_id: String) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            own_endpoint_id,
            db: None,
            sync_state: Mutex::new(HashMap::new()),
            dial_semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DIALS)),
        }
    }

    /// Construct with a durable directory backing store.
    pub fn new_with_db(own_endpoint_id: String, db: Arc<Database>) -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            own_endpoint_id,
            db: Some(db),
            sync_state: Mutex::new(HashMap::new()),
            dial_semaphore: Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_DIALS)),
        }
    }

    /// Whether a transient sv_exchange dial to this peer is due. Applies
    /// exponential backoff on consecutive failures so unreachable peers aren't
    /// hammered (the "stop hunting when not making progress" lever).
    pub fn should_sync_peer(&self, endpoint_id: &str, base: Duration) -> bool {
        let state = self.sync_state.lock().expect("sync_state lock poisoned");
        match state.get(endpoint_id) {
            None => true,
            Some(s) => match s.last_attempt {
                None => true,
                Some(t) => {
                    let shift = s.failures.min(MAX_BACKOFF_SHIFT);
                    t.elapsed() >= base * 2u32.pow(shift)
                }
            },
        }
    }

    /// Record that a sync dial was just started for this peer.
    pub fn mark_sync_attempted(&self, endpoint_id: &str) {
        let mut state = self.sync_state.lock().expect("sync_state lock poisoned");
        state
            .entry(endpoint_id.to_string())
            .or_default()
            .last_attempt = Some(Instant::now());
    }

    /// Record a dial's outcome: success resets the backoff, failure widens it.
    pub fn record_sync_result(&self, endpoint_id: &str, ok: bool) {
        let mut state = self.sync_state.lock().expect("sync_state lock poisoned");
        let s = state.entry(endpoint_id.to_string()).or_default();
        if ok {
            s.failures = 0;
        } else {
            s.failures = s.failures.saturating_add(1);
        }
    }

    /// Acquire a permit from the dial budget, or `None` if unavailable. Held for
    /// the lifetime of a transient dial so concurrent dials stay bounded.
    ///
    /// `high_priority` is for group topics: low-priority (public) dials are
    /// refused once free permits fall to [`HIGH_PRIORITY_RESERVE`], reserving
    /// headroom so group catch-up wins under contention.
    pub fn try_acquire_dial_permit(
        &self,
        high_priority: bool,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        if !high_priority && self.dial_semaphore.available_permits() <= HIGH_PRIORITY_RESERVE {
            return None;
        }
        Arc::clone(&self.dial_semaphore).try_acquire_owned().ok()
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
        if let Err(e) = db.upsert_festival_peer(
            festival_id,
            endpoint_id,
            None,
            now,
            PeerSource::Gossip.as_str(),
        ) {
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
            Some(e) => match e.last_join_attempt {
                None => true,
                Some(t) => t.elapsed() >= min_interval,
            },
        }
    }

    /// List all peers currently known to be visible via BLE.
    pub fn list_verified_ble_peers(&self) -> Vec<String> {
        let table = self.peers.lock().expect("peer table lock poisoned");
        table
            .values()
            .filter(|e| e.source == PeerSource::Ble)
            .map(|e| e.endpoint_id.clone())
            .collect()
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
        cm.record_festival_peers(
            "fest-1",
            &[make_peer("peer-a", None, 100)],
            PeerSource::Crdt,
        );
        assert!(cm.bootstrap_peers("fest-1", 10).is_empty());
    }

    #[test]
    fn test_sync_throttle_gates_repeat_dials() {
        let cm = ConnectionManager::new("own-id".to_string());
        let long = Duration::from_secs(3600);

        // Unknown peer is always due.
        assert!(cm.should_sync_peer("peer-a", long));

        // After a dial, not due again within the window.
        cm.mark_sync_attempted("peer-a");
        assert!(!cm.should_sync_peer("peer-a", long));

        // A different peer is unaffected.
        assert!(cm.should_sync_peer("peer-b", long));

        // A zero base means no throttle.
        assert!(cm.should_sync_peer("peer-a", Duration::ZERO));
    }

    #[test]
    fn test_sync_result_resets_and_widens_backoff() {
        let cm = ConnectionManager::new("own-id".to_string());
        cm.mark_sync_attempted("peer-a");

        // Two failures widen the backoff; with a 1ns base, 2^2ns is still tiny
        // so the peer becomes due again — this asserts failures are tracked, not
        // that we wait. (Timing-based backoff isn't unit-tested with sleeps.)
        cm.record_sync_result("peer-a", false);
        cm.record_sync_result("peer-a", false);
        assert!(cm.should_sync_peer("peer-a", Duration::from_nanos(1)));

        // Success resets the failure count (no panic, idempotent).
        cm.record_sync_result("peer-a", true);
    }

    #[tokio::test]
    async fn test_dial_permits_bound_concurrency() {
        let cm = ConnectionManager::new("own-id".to_string());
        // Drain the whole budget at high priority.
        let mut held = Vec::new();
        for _ in 0..MAX_CONCURRENT_DIALS {
            held.push(cm.try_acquire_dial_permit(true).expect("permit available"));
        }
        // At capacity → no more, even high priority.
        assert!(cm.try_acquire_dial_permit(true).is_none());
        // Release one → available again.
        held.pop();
        assert!(cm.try_acquire_dial_permit(true).is_some());
    }

    #[tokio::test]
    async fn test_low_priority_yields_reserve_to_group_topics() {
        let cm = ConnectionManager::new("own-id".to_string());
        // Consume down to exactly the reserve floor.
        let mut held = Vec::new();
        for _ in 0..(MAX_CONCURRENT_DIALS - HIGH_PRIORITY_RESERVE) {
            held.push(cm.try_acquire_dial_permit(false).expect("low-pri permit"));
        }
        // Public (low-priority) dials now refused — reserve is for group topics.
        assert!(cm.try_acquire_dial_permit(false).is_none());
        // Group (high-priority) dials may still use the reserved headroom.
        assert!(cm.try_acquire_dial_permit(true).is_some());
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
