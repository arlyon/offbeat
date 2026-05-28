//! Offline group-member discovery via rotating BLE beacons.
//!
//! In an offline crowd, iroh-gossip cannot find your ~3 group members among
//! 100 strangers — it has no notion of "peer X is likely on topic Y". We solve
//! this *connectionlessly*: each node advertises, as BLE service UUIDs, a set
//! of rotating beacons derived from its groups' keys. A scanner intersects the
//! observed service UUIDs with the beacons it would expect for its own groups.
//! Because co-members share the `group_key`, they derive identical beacons —
//! a cheap private-set intersection — while non-members see only opaque,
//! per-epoch-rotating bytes (unlinkable, untrackable, leak-expiring).
//!
//! A match is a *candidate* only: the 96-bit truncation admits rare false
//! positives, and matching does not prove membership. The caller connects on a
//! match and runs a full GATT membership handshake before trusting the peer.
//!
//! This module is pure logic (no BLE I/O) so it is fully unit-testable without
//! hardware. The transport layer supplies `now_secs` and the observed UUIDs.

use std::collections::HashMap;

use uuid::Uuid;

/// 4-byte tag marking a UUID as an offbeat group beacon, distinguishing it from
/// the iroh dial-key UUID (whose prefix is `0x69 0x72 0x6f 0x00`, i.e. "iro\0").
/// Beacons use "obg\0".
pub const BEACON_PREFIX: [u8; 4] = [b'o', b'b', b'g', 0x00];

/// How long a beacon value stays valid. 15 minutes balances unlinkability
/// (frequent rotation) against the clock-skew tolerance the ±1-epoch window
/// (see [`advertised_beacons`]) provides — roughly one full epoch of skew.
pub const EPOCH_LEN_SECS: u64 = 15 * 60;

/// A 128-bit beacon, advertised/observed as a BLE service UUID (big-endian
/// byte order, matching `uuid::Uuid::as_bytes`).
pub type Beacon = [u8; 16];

/// Epoch index for a unix timestamp (seconds).
#[must_use]
pub fn epoch_at(now_secs: u64) -> u64 {
    now_secs / EPOCH_LEN_SECS
}

/// True if a UUID carries the offbeat beacon prefix. Lets the scan path cheaply
/// pre-filter candidate beacons (dropping dial keys and unrelated services)
/// before the set lookup in [`match_observed`].
#[must_use]
pub fn is_beacon(uuid: &Beacon) -> bool {
    uuid[..4] == BEACON_PREFIX
}

/// Derive the rotating beacon for a group at a given epoch:
/// `BEACON_PREFIX || truncate(blake3_keyed(group_key, epoch_le), 12)`.
///
/// blake3 keyed-hash is a MAC over the 32-byte group key, so co-members derive
/// identical beacons and outsiders cannot forge or link them across epochs.
#[must_use]
pub fn group_beacon(group_key: &[u8; 32], epoch: u64) -> Beacon {
    let mac = blake3::keyed_hash(group_key, &epoch.to_le_bytes());
    let mac = mac.as_bytes();
    let mut beacon = [0u8; 16];
    beacon[..4].copy_from_slice(&BEACON_PREFIX);
    beacon[4..].copy_from_slice(&mac[..12]);
    beacon
}

/// Beacons to advertise for our groups right now: the current epoch's beacon
/// plus the previous epoch's, per group. Advertising (and matching) both means
/// two nodes whose epoch indices differ by at most 1 always share a beacon, so
/// clock skew up to ~one epoch never hides co-members from each other.
#[must_use]
pub fn advertised_beacons(group_keys: &[[u8; 32]], now_secs: u64) -> Vec<Beacon> {
    let epoch = epoch_at(now_secs);
    let mut out = Vec::with_capacity(group_keys.len() * 2);
    for key in group_keys {
        out.push(group_beacon(key, epoch));
        if epoch > 0 {
            out.push(group_beacon(key, epoch - 1));
        }
    }
    out
}

/// A hit between an observed advertised UUID and one of our groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeaconMatch {
    /// Index into the `group_keys` slice passed to [`match_observed`].
    pub group_index: usize,
    /// The matched beacon value (an entry from `observed`).
    pub beacon: Beacon,
}

/// Intersect observed advertised UUIDs with the current+previous-epoch beacons
/// of our groups. Returns one [`BeaconMatch`] per hit. Non-group UUIDs (the
/// peer's dial key, other groups' beacons, unrelated services) miss.
///
/// Matches are discovery *candidates*, not membership proofs — verify over GATT
/// before trusting.
#[must_use]
pub fn match_observed(
    group_keys: &[[u8; 32]],
    observed: &[Beacon],
    now_secs: u64,
) -> Vec<BeaconMatch> {
    let epoch = epoch_at(now_secs);
    let mut by_beacon: HashMap<Beacon, usize> = HashMap::with_capacity(group_keys.len() * 2);
    for (i, key) in group_keys.iter().enumerate() {
        by_beacon.insert(group_beacon(key, epoch), i);
        if epoch > 0 {
            by_beacon.insert(group_beacon(key, epoch - 1), i);
        }
    }
    observed
        .iter()
        .filter_map(|uuid| {
            by_beacon.get(uuid).map(|&group_index| BeaconMatch {
                group_index,
                beacon: *uuid,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Transport boundary: bridges our `[u8; 16]` beacons to the `uuid::Uuid` the
// BLE transport advertises and observes. Beacon bytes are big-endian, matching
// `uuid::Uuid::{from_bytes, as_bytes}` and the transport's own key-UUID scheme.
// ---------------------------------------------------------------------------

/// Beacons to advertise for our groups right now, as `uuid::Uuid`s ready for
/// `BleTransport::set_discovery_beacons`.
#[must_use]
pub fn beacon_uuids(group_keys: &[[u8; 32]], now_secs: u64) -> Vec<Uuid> {
    advertised_beacons(group_keys, now_secs)
        .into_iter()
        .map(Uuid::from_bytes)
        .collect()
}

/// Match a scanned advertisement's service UUIDs against our groups' beacons.
/// Cheaply drops non-beacon UUIDs (the peer's dial key, unrelated services)
/// before the set intersection. Matches are candidates — verify over GATT.
#[must_use]
pub fn match_advert_services(
    group_keys: &[[u8; 32]],
    services: &[Uuid],
    now_secs: u64,
) -> Vec<BeaconMatch> {
    let observed: Vec<Beacon> = services
        .iter()
        .map(|u| *u.as_bytes())
        .filter(is_beacon)
        .collect();
    match_observed(group_keys, &observed, now_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_A: [u8; 32] = [1u8; 32];
    const KEY_B: [u8; 32] = [2u8; 32];

    // A timestamp comfortably inside a non-zero epoch.
    const NOW: u64 = 100 * EPOCH_LEN_SECS + 42;

    #[test]
    fn beacon_is_deterministic() {
        assert_eq!(group_beacon(&KEY_A, 100), group_beacon(&KEY_A, 100));
    }

    #[test]
    fn beacon_carries_prefix() {
        let b = group_beacon(&KEY_A, 100);
        assert_eq!(&b[..4], &BEACON_PREFIX);
        assert!(is_beacon(&b));
    }

    #[test]
    fn beacon_differs_by_group_and_epoch() {
        assert_ne!(group_beacon(&KEY_A, 100), group_beacon(&KEY_B, 100));
        assert_ne!(group_beacon(&KEY_A, 100), group_beacon(&KEY_A, 101));
    }

    #[test]
    fn epoch_boundaries() {
        assert_eq!(epoch_at(0), 0);
        assert_eq!(epoch_at(EPOCH_LEN_SECS - 1), 0);
        assert_eq!(epoch_at(EPOCH_LEN_SECS), 1);
    }

    #[test]
    fn advertised_set_has_current_and_previous_per_group() {
        let beacons = advertised_beacons(&[KEY_A, KEY_B], NOW);
        assert_eq!(beacons.len(), 4);
        let epoch = epoch_at(NOW);
        assert!(beacons.contains(&group_beacon(&KEY_A, epoch)));
        assert!(beacons.contains(&group_beacon(&KEY_A, epoch - 1)));
        assert!(beacons.contains(&group_beacon(&KEY_B, epoch)));
        assert!(beacons.contains(&group_beacon(&KEY_B, epoch - 1)));
    }

    #[test]
    fn advertised_set_handles_epoch_zero() {
        // At epoch 0 there is no previous epoch — only the current beacon.
        let beacons = advertised_beacons(&[KEY_A], 5);
        assert_eq!(beacons.len(), 1);
        assert_eq!(beacons[0], group_beacon(&KEY_A, 0));
    }

    #[test]
    fn matches_co_member_same_epoch() {
        let advertised = advertised_beacons(&[KEY_A], NOW);
        let matches = match_observed(&[KEY_A], &advertised, NOW);
        assert!(!matches.is_empty());
        assert!(matches.iter().all(|m| m.group_index == 0));
    }

    #[test]
    fn matches_across_one_epoch_skew_both_directions() {
        // Advertiser one epoch ahead of scanner.
        let adv_ahead = advertised_beacons(&[KEY_A], NOW + EPOCH_LEN_SECS);
        assert!(!match_observed(&[KEY_A], &adv_ahead, NOW).is_empty());

        // Advertiser one epoch behind scanner.
        let adv_behind = advertised_beacons(&[KEY_A], NOW - EPOCH_LEN_SECS);
        assert!(!match_observed(&[KEY_A], &adv_behind, NOW).is_empty());
    }

    #[test]
    fn no_match_two_epochs_apart() {
        let adv = advertised_beacons(&[KEY_A], NOW + 2 * EPOCH_LEN_SECS);
        assert!(match_observed(&[KEY_A], &adv, NOW).is_empty());
    }

    #[test]
    fn non_member_does_not_match() {
        // A scanner in group B observes group A's advertisement.
        let adv_a = advertised_beacons(&[KEY_A], NOW);
        assert!(match_observed(&[KEY_B], &adv_a, NOW).is_empty());
    }

    #[test]
    fn dial_key_uuid_is_not_a_beacon_and_does_not_match() {
        // iroh dial-key UUID prefix is "iro\0"; it must never match a beacon.
        let mut dial_key = [7u8; 16];
        dial_key[..4].copy_from_slice(&[b'i', b'r', b'o', 0x00]);
        assert!(!is_beacon(&dial_key));
        assert!(match_observed(&[KEY_A], &[dial_key], NOW).is_empty());
    }

    #[test]
    fn beacon_uuids_roundtrip_through_match() {
        // What we advertise is exactly what a co-member scanner matches.
        let advertised = beacon_uuids(&[KEY_A], NOW);
        let matches = match_advert_services(&[KEY_A], &advertised, NOW);
        assert!(!matches.is_empty());
        assert!(matches.iter().all(|m| m.group_index == 0));
    }

    #[test]
    fn match_advert_services_ignores_non_beacon_uuids() {
        // A dial-key UUID mixed in with a real beacon: only the beacon matches.
        let mut dial_key = [0u8; 16];
        dial_key[..4].copy_from_slice(&[b'i', b'r', b'o', 0x00]);
        let beacon = Uuid::from_bytes(group_beacon(&KEY_A, epoch_at(NOW)));
        let services = vec![Uuid::from_bytes(dial_key), beacon];
        let matches = match_advert_services(&[KEY_A], &services, NOW);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].beacon, *beacon.as_bytes());
    }

    #[test]
    fn reports_correct_group_index_with_multiple_groups() {
        let groups = [KEY_A, KEY_B];
        let observed = vec![group_beacon(&KEY_B, epoch_at(NOW))];
        let matches = match_observed(&groups, &observed, NOW);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].group_index, 1);
        assert_eq!(matches[0].beacon, observed[0]);
    }
}
