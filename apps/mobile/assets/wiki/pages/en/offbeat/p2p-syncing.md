---
{
  "schemaVersion": 1,
  "id": "offbeat.p2p-syncing",
  "locale": "en",
  "title": "OFFBEAT P2P syncing",
  "summary": "Understand local-first state, relay and peer routes, convergence, sync indicators, trust boundaries, and recovery.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["peer to peer", "P2P", "sync status", "CRDT sync", "gossip"],
  "tags": ["P2P", "sync", "offline", "CRDT", "privacy"],
  "generatedRefs": [],
  "priority": "high",
  "order": 820,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Resource model and visibility",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/resource.rs"
    },
    {
      "title": "CRDT document management",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/doc_manager.rs"
    },
    {
      "title": "Sync orchestrator",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/sync.rs"
    },
    {
      "title": "Direct catch-up protocol",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/sync_protocol.rs"
    },
    {
      "title": "Peer connection manager",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/connection_manager.rs"
    },
    {
      "title": "Mobile network and outbox wiring",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/mod.rs"
    }
  ]
}
---
# OFFBEAT P2P syncing

OFFBEAT is local first. Accepted festival documents, group state, chat and personal choices are read from local storage. Network routes are used to exchange what is missing, not to make every screen depend on a live server response.

## Current support

- **Normal route:** the Festival Durable Object WebSocket relay carries live gossip and catch-up when internet access is available.
- **Integrated local route, field validation still limited:** OFFBEAT's iroh Bluetooth transport discovers nearby OFFBEAT peers and can form gossip connections. Hardware, operating-system and background behavior can still limit it.
- **Planned only:** Wi-Fi Aware and Wi-Fi Direct are not usable routes in the current app.
- **Debug only:** OFFBEAT over a Meshtastic radio is an Android test-rig path, not a normal sync route.

No one route is promised. The app may have local data even when every route is unavailable.

## Prerequisites

- A festival must already be known on the device.
- Private group sync requires local possession of that group's 32-byte group key, normally obtained through an invite.
- Nearby peers need a compatible OFFBEAT build, access to the same public festival, and any required Bluetooth permissions.
- A relay route needs connectivity and a valid festival-bound session.

Peer discovery does not grant access. Resource registration and cryptographic checks still decide what can be applied.

## What sync does

OFFBEAT has two main data shapes:

- Yrs CRDT documents for festival and group state.
- Append logs for public and group chat.

On a capable route, peers exchange a state vector and send only missing CRDT state. Reapplying CRDT information is designed to converge rather than create a second independent copy. Chat catch-up uses per-writer high-water information and bounded pages.

A live gossip subscription carries later updates. When a peer appears, OFFBEAT may attempt transient catch-up. Failed direct attempts use bounded concurrency and widening backoff instead of continuous dialing.

## Partitions and eventual convergence

Two disconnected devices can temporarily show different state. When a valid route returns, compatible CRDT changes can merge and missing log entries can be requested. "Eventual" does not mean immediate or guaranteed:

- A peer may never come back into range.
- The relay may be unavailable.
- A low-bandwidth route may suppress or cap some payloads.
- Chat history is bounded and private group chat catch-up over a direct peer is not a blanket history service.
- Application or platform shutdown can stop background work.

Group-state changes are persisted locally and queued for relay retry when necessary. Normal group chat is saved locally and sent best effort, but it does not yet have a restart-safe outbound retry queue. A message appearing in your own chat therefore does not prove another member received it.

## Trust and privacy

Public and private resources have different boundaries:

- Festival state is public but must be signed by the configured festival authority. A relay or peer cannot make an unsigned lineup trusted.
- Group state and group chat are encrypted with the group key. The Durable Object can carry opaque ciphertext without learning plaintext.
- The group key is access. Anyone who obtains an invite containing it can decrypt current group resources.
- iroh endpoint identity authenticates a transport peer, not festival authority or group membership.

Metadata such as endpoint IDs, topic activity, timing and traffic size can still be visible to transport infrastructure or nearby observers. Encryption is not anonymity.

## Reading the indicators

The connection drawer separates the WebSocket relay from Bluetooth LE and shows peer, traffic and resource counters.

- `ONLINE` for Bluetooth means the transport is active, not that a peer is connected or caught up.
- A discovered or prefix-matched Bluetooth device is not yet a verified iroh peer.
- A peer count describes current transport or gossip state, not the number of people who have received an update.
- `SYNCING` means work is in progress.
- Sent and received counters show traffic, not semantic freshness or read receipts.
- The current resource status does not provide a reliable user-facing `last synced` timestamp for every resource.

Use the screen as diagnostics, not as proof that everyone has the same state.

## Offline behavior

Without any route, previously accepted lineup, weather, stars, groups, check-ins and stored chat remain readable according to their individual feature limits. Local changes should be treated separately:

- Personal stars persist immediately.
- Group-state mutations persist and have a relay retry path.
- Group chat persists on the sender but remote delivery is best effort.
- Public chat has its own durable outbox, but still has no human read receipt.

## Constraints

- OFFBEAT does not discover a never-seen festival from peers.
- Wi-Fi Aware and full iroh over Meshtastic are unavailable.
- Bulk history is not sent over constrained Meshtastic links.
- Transport switching does not weaken signature or encryption checks.
- Duplicate network delivery can happen. CRDT application and stored message identities limit duplicate logical effects, but full cross-route field evidence remains incomplete.

## Troubleshooting

1. Confirm the expected data already exists locally before testing offline use.
2. Open the connection drawer and distinguish relay status, Bluetooth transport state and verified peers.
3. On Android, grant scan, connect and advertise permissions and turn Bluetooth on.
4. Keep both devices awake and the app open while diagnosing a local route.
5. Confirm both devices selected the same festival. For groups, confirm both joined the same group.
6. Wait for backoff and catch-up rather than repeatedly clearing or recreating state.
7. Use `NUDGE` or `RESTART` in the connection drawer only when the transport appears stuck.
8. Do not clear app storage or leave and rejoin a group as routine sync troubleshooting. Those actions can remove local keys or private history.

See [Bluetooth sync](wiki:offbeat.bluetooth-sync), [Groups](wiki:offbeat.groups), and [OFFBEAT lineup](wiki:offbeat.lineup) for feature-specific limits.
