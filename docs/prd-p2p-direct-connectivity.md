# PRD: Direct P2P Connectivity & Multi-Path Bootstrap

## Introduction

All sync in Offbeat currently flows through the WebSocket relay to the Festival Durable Object. iroh-gossip is instantiated but idle (empty bootstrap lists), and BLE discovers nearby peers but cannot connect them to gossip topics. This results in a single point of failure — if the WS relay is unreachable, no sync occurs, even between two phones standing next to each other.

This PRD adds direct peer-to-peer connectivity via multiple independent bootstrap paths, reducing dependency on the central relay and enabling fully offline peer discovery and sync.

## Goals

| # | Goal | Metric |
|---|------|--------|
| G1 | Enable direct P2P connections between festival attendees via QUIC hole-punching and BLE | gossip `active_peers > 0` without WS relay |
| G2 | Provide four independent bootstrap paths (BLE identity, CRDT+BLE, CRDT+internet, DO relay) | Each path independently achieves sync |
| G3 | Support fully offline peer discovery via BLE alone | Two phones in airplane mode can sync |
| G4 | Unify sync protocol around iroh-gossip, making the DO "just another peer" | Single code path for all sync |
| G5 | Design for 10s–100s of concurrent peers, architect for 10,000s | CRDT peer list handles 10k entries |

## Target Audience

Festival attendees using the Offbeat mobile app. Key scenarios:

- **Poor 4G**: crowded festival, cell towers overloaded — BLE and local WiFi are more reliable
- **No internet**: remote festival site, camping areas with no signal
- **New user**: just installed the app, standing next to someone who has festival data
- **Day 3 attendee**: has stale data, needs to catch up from peers

## User Stories

### US-1: BLE Offline Discovery (fully offline, no CRDT, no DO)
As a festival attendee with no internet, I want my phone to automatically discover nearby peers via Bluetooth, learn their identity, find shared festivals/groups, and sync — without ever contacting the server.

**Acceptance test:** Two phones in airplane mode, one with festival data, one fresh install. Within 30 seconds of BLE discovery, the fresh phone has the festival lineup.

### US-2: CRDT-Bootstrapped Direct Connection
As a festival attendee whose phone has synced the peer list, I want my phone to hole-punch directly to other attendees over the internet, so sync doesn't depend on the central relay.

**Acceptance test:** Two phones on different WiFi networks, both with CRDT peer list containing each other's EndpointId. Gossip `active_peers` reaches 1 within 10 seconds.

### US-3: Automatic Peer Registration
As a festival attendee, I want my phone to periodically register my presence with the festival server, so other attendees can discover and connect to me directly.

**Acceptance test:** After 60 minutes with the app foregrounded, the festival CRDT contains my EndpointId with a `last_seen` within the last 60 minutes.

### US-4: Seamless Multi-Transport
As a festival attendee, I want my phone to automatically use the best available transport (internet, BLE, or relay) without any manual configuration.

**Acceptance test:** Phone connected via relay, enters BLE range of a direct peer — gossip traffic shifts to BLE without user action.

## System Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                       Connection Manager                            │
│                                                                     │
│  Inputs:                                                            │
│  ├─ BLE transport: discovered KeyPrefixes + EndpointId char reads  │
│  ├─ CRDT peer list: EndpointId + relay_url + last_seen             │
│  └─ Resource registry: active gossip topics                        │
│                                                                     │
│  Bootstrap Paths (all independent, none required):                  │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ Path 1: BLE GATT read → full EndpointId → ZK handshake     │    │
│  │         (fully offline, no CRDT needed)                     │    │
│  │                                                             │    │
│  │ Path 2: CRDT EndpointId + BLE prefix match → join_peers    │    │
│  │         (offline once CRDT is synced)                       │    │
│  │                                                             │    │
│  │ Path 3: CRDT EndpointId + relay URL → hole punch           │    │
│  │         (internet required, no BLE needed)                  │    │
│  │                                                             │    │
│  │ Path 4: DO/WS relay → gossip bridge                        │    │
│  │         (always-reachable fallback, store-and-forward)      │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                     │
│  Outputs:                                                           │
│  ├─ endpoint.add_node_addr(addr)                                   │
│  └─ gossip.join_peers([endpoint_ids])                              │
│                                                                     │
│  Ticks:                                                             │
│  ├─ peer_discovery_tick (~10s)  — match BLE ↔ CRDT, read chars     │
│  ├─ heartbeat_tick (~60min)     — POST EndpointId to DO            │
│  └─ reconnect_tick (~10s)       — re-join dropped gossip peers     │
└─────────────────────────────────────────────────────────────────────┘
```

### Bootstrap Path Detail: BLE Identity Exchange (Path 1)

This is the fully-offline path for two strangers:

```
Phone A                                    Phone B
   │                                          │
   ├─ BLE advertise KeyPrefix_A              ├─ BLE advertise KeyPrefix_B
   │                                          │
   ├─ Scan: discover KeyPrefix_B             ├─ Scan: discover KeyPrefix_A
   │                                          │
   ├─ No CRDT match for prefix_B             │
   │                                          │
   ├─ Read GATT char 69726f06 from B ────────┤
   │  (returns full 32-byte EndpointId_B)     │
   │                                          │
   ├─ endpoint.add_node_addr(B, ble_addr)     │
   ├─ QUIC connect over BLE ─────────────────►│
   │  (TLS handshake verifies both identities)│
   │                                          │
   ├─ Zero-knowledge group handshake ◄───────►├─ (group_sync.rs)
   │  (discover shared festivals/groups)      │
   │                                          │
   ├─ gossip.join_peers([B]) on shared topics │
   │                                          │
   └─ Bidirectional sync active ◄────────────►└─
```

## Data Schema

### PeerInfo (in festival CRDT document)

Stored under `peers/` prefix in the festival Yrs document, signed by the DO:

```json
{
  "peers": {
    "<endpoint_id_hex>": {
      "relay_url": "https://use1-1.relay.iroh.network.",
      "last_seen": 1748000000,
      "user_id": "webauthn_user_id_here"
    }
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| key | string | 64-char hex EndpointId (map key) |
| `relay_url` | string \| null | Client-reported home relay URL |
| `last_seen` | u64 | Unix timestamp (seconds), set by DO |
| `user_id` | string | WebAuthn-verified user identity |

**Size at scale:** ~120 bytes/entry. 10,000 entries = ~1.2 MB in the CRDT.

### BLE GATT Characteristic (new)

Added to the existing `IROH_SERVICE_UUID` service:

```
69726f06-8e45-4c2c-b3a5-331f3098b5c2  (IROH_ENDPOINT_ID_CHAR_UUID)
Properties: READ
Value: 32 bytes — raw Ed25519 public key (EndpointId)
```

### PeerEntry (client-side, in-memory)

```rust
struct PeerEntry {
    endpoint_id: EndpointId,
    relay_url: Option<String>,
    last_seen: u64,
    source: PeerSource,         // Crdt | Ble | Gossip
    ble_prefix_match: bool,     // true if BLE scan_hint matches this peer
    gossip_status: GossipStatus, // Unknown | Joining | Direct | Stale
}

enum PeerSource {
    Crdt,       // Learned from festival CRDT peer list
    Ble,        // Learned via BLE GATT characteristic read
    Gossip,     // Learned from incoming gossip message
}
```

## Interface Definitions

### REST: Peer Checkin

```
POST /festivals/{festival_id}/checkin
Authorization: Bearer <webauthn_token>
Content-Type: application/json

Request:
{
  "endpoint_id": "aabbccdd...",    // 64-char hex
  "relay_url": "https://..."       // nullable, client's home relay
}

Response 200:
{
  "ttl": 7200,                     // seconds until this entry expires
  "peer_count": 42                 // current active peer count
}

Response 401: Unauthorized (invalid/expired WebAuthn token)
Response 404: Festival not found
```

### Rust: Connection Manager API

```rust
impl ConnectionManager {
    /// Create a new connection manager.
    pub fn new(
        endpoint: iroh::Endpoint,
        gossip: Gossip,
        ble_transport: Option<Arc<BleTransport>>,
        resource_registry: Arc<RwLock<ResourceRegistry>>,
        doc_manager: Arc<DocManager>,
    ) -> Self;

    /// Start all background ticks. Returns a handle to stop them.
    pub async fn start(&self) -> ConnectionManagerHandle;

    /// Notify the manager that the CRDT peer list has been updated.
    pub fn on_peer_list_updated(&self, peers: Vec<PeerInfo>);

    /// Get the current state of all known peers.
    pub fn peer_snapshot(&self) -> Vec<PeerEntry>;
}
```

### Rust: BLE Transport Extension

```rust
impl BleTransport {
    /// Read the full EndpointId from a BLE-discovered peer's GATT characteristic.
    /// Returns None if the peer doesn't serve the characteristic or read fails.
    pub async fn read_endpoint_id(&self, device_id: &DeviceId) -> Option<EndpointId>;
}
```

## Functional Requirements

### FR-1: CRDT Peer List — Server

**FR-1.1**: The Festival DO SHALL accept `POST /festivals/{id}/checkin` with a WebAuthn-authenticated request containing an `endpoint_id` and optional `relay_url`.

**FR-1.2**: The DO SHALL write the peer entry into the festival CRDT under `peers/{endpoint_id_hex}` with `last_seen` set to the current Unix timestamp.

**FR-1.3**: The DO SHALL prune peer entries where `last_seen` is older than 2 hours, running on each checkin request and on a periodic timer (every 15 minutes).

**FR-1.4**: The CRDT peer list update SHALL be signed by the DO's Ed25519 key and distributed via the existing festival state gossip topic.

**Testable acceptance criteria:**
- `curl -X POST /festivals/{id}/checkin -d '{"endpoint_id":"aa.."}' -H 'Authorization: Bearer ...'` → 200 with `ttl` and `peer_count`
- Festival CRDT read returns the peer entry with correct `last_seen`
- Entry disappears after 2 hours without re-checkin
- `pnpm -F @offbeat/server test` passes

### FR-2: CRDT Peer List — Client

**FR-2.1**: `DocManager` SHALL parse `peers/*` entries from the festival CRDT document into `Vec<PeerInfo>`.

**FR-2.2**: The parser SHALL filter out the local node's own EndpointId.

**FR-2.3**: The parser SHALL tolerate malformed entries (skip them, log a warning).

**Testable acceptance criteria:**
- `cargo test peer_list_parse` — round-trip write/read of peer entries in a Yrs doc
- Own EndpointId is excluded
- Malformed entries don't crash the parser

### FR-3: BLE EndpointId Characteristic

**FR-3.1**: The BLE transport SHALL add a READ-only GATT characteristic (`69726f06-8e45-4c2c-b3a5-331f3098b5c2`) to the `IROH_SERVICE_UUID` service, containing the node's full 32-byte EndpointId.

**FR-3.2**: `BleTransport` SHALL expose a `read_endpoint_id(device_id)` method that reads this characteristic from a connected peer.

**FR-3.3**: The characteristic SHALL be present on all platforms (iOS, Android, desktop).

**Testable acceptance criteria:**
- `cargo test -p iroh-ble-transport endpoint_id_characteristic` — verify characteristic value matches node pubkey
- `read_endpoint_id` returns correct 32 bytes from a peer in test harness

### FR-4: Connection Manager

**FR-4.1** (`peer_discovery_tick`, ~10s): The connection manager SHALL:
1. Get BLE-discovered KeyPrefixes from `BleTransport::snapshot_peers()`
2. Match against EndpointIds from the CRDT peer list (first 12 bytes)
3. For matches: call `endpoint.add_node_addr()` with BLE + relay URL, then `gossip.join_peers()`
4. For unmatched BLE peers (no CRDT entry): call `read_endpoint_id()` via GATT, then run the zero-knowledge handshake (`group_sync.rs`) to discover shared topics

**FR-4.2** (`heartbeat_tick`, ~60min): The connection manager SHALL POST the node's EndpointId and home relay URL to the Festival DO checkin endpoint while the app is foregrounded.

**FR-4.3** (`reconnect_tick`, ~10s): The connection manager SHALL re-join peers that have dropped off gossip topics, following the chat app's pattern:
- Only dial peers whose KeyPrefix is in the BLE scan_hint table (if BLE available)
- For internet-only peers (no BLE): dial if they have a relay URL in the CRDT
- Throttle: don't re-join the same peer within 5 seconds

**FR-4.4**: For internet peers (not BLE-visible), the connection manager SHALL call `endpoint.add_node_addr()` with the relay URL from the CRDT, letting iroh handle QUIC hole-punching natively.

**FR-4.5**: The connection manager SHALL emit events when peer status changes (discovered, connected, direct, stale, lost) for UI consumption via FRB.

**Testable acceptance criteria:**
- `cargo test connection_manager_ble_match` — mock BLE prefix + CRDT entry → `join_peers` called
- `cargo test connection_manager_heartbeat` — heartbeat fires within configured interval
- `cargo test connection_manager_reconnect` — dropped peer is re-joined on next tick

### FR-5: Gossip Protocol Bridge (WS Relay)

**FR-5.1**: The Festival DO SHALL speak the iroh-gossip wire protocol (protobuf `GossipEnvelope` messages) over WebSocket, replacing the current custom sync protocol.

**FR-5.2**: The Festival DO SHALL have a deterministic EndpointId derived from the festival's signing key, so clients can identify it as a gossip peer.

**FR-5.3**: The client SHALL implement a `WsGossipTransport` that wraps WebSocket as a gossip-compatible transport, making the DO appear as a regular gossip peer to `GossipManager`.

**FR-5.4**: The DO SHALL buffer recent gossip messages per topic (last N messages or last T seconds) and replay them to late-joining peers (store-and-forward).

**FR-5.5**: The existing `PeerConnection` trait, `WsRelaySink`, and custom WS sync logic SHALL be removed once the gossip bridge is operational.

**Testable acceptance criteria:**
- Client connects to DO via WS, subscribes to topic, receives messages in `GossipEnvelope` format
- Late joiner receives buffered messages
- `SyncOrchestrator` no longer references `PeerConnection` trait
- `pnpm check` passes

### FR-6: Persist EndpointId Across Restarts

**FR-6.1**: The iroh `SecretKey` SHALL be persisted to the local SQLite database on first run and reloaded on subsequent launches, so the EndpointId remains stable across app restarts.

**Testable acceptance criteria:**
- `cargo test secret_key_persistence` — create node, record EndpointId, recreate node with same db path, verify same EndpointId

## Non-Functional Requirements

### NFR-1: Scale
- CRDT peer list: 10,000 entries (~1.2 MB) must sync within 5 seconds on a 4G connection
- Connection manager: processing 1,000 CRDT entries against BLE scan hints completes in <100ms
- `join_peers` calls capped at 20 peers per tick to avoid overwhelming iroh's connection table

### NFR-2: Performance
- BLE EndpointId characteristic read: <500ms (single GATT read)
- First direct peer connection after CRDT sync: <10s on WiFi, <30s on 4G
- BLE-only peer discovery to first sync: <30s

### NFR-3: Battery
- `peer_discovery_tick` (10s) is adaptive: runs at 10s when BLE peers are visible, slows to 30s when idle
- `heartbeat_tick` at 60 minutes is negligible
- `reconnect_tick` (10s) only processes peers with active scan hints — no wasted dials

### NFR-4: Privacy
- EndpointId is an ephemeral device key, not tied to real-world identity (but persisted per-install)
- CRDT peer list associates EndpointId → user_id, visible to all festival attendees who have the CRDT
- BLE EndpointId characteristic exposes the full key to anyone within ~10m BLE range
- The zero-knowledge handshake (group_sync.rs) ensures group membership is not leaked during BLE discovery

## Design Considerations

1. **KeyPrefix uniqueness**: 12 bytes = 96 bits. At 10,000 peers, birthday collision probability is ~5 x 10^-20. Safe to treat as unique.

2. **iroh connection table memory**: iroh maintains per-peer state. At 10,000 `add_node_addr` calls, memory usage may be significant. The connection manager should only add peers it intends to actively connect to — BLE-visible peers plus a bounded sample of internet peers.

3. **Pkarr/DNS as complement**: `presets::N0` enables pkarr publishing by default. If enabled, any peer who knows an EndpointId can resolve its relay URL via DNS without the CRDT. This provides a third relay-discovery path alongside CRDT and BLE, but requires internet. Store relay_url in CRDT for offline resilience.

4. **Race between paths**: Multiple bootstrap paths may try to connect to the same peer simultaneously (e.g., BLE + internet). iroh handles this gracefully — it deduplicates connections and picks the best transport.

5. **CRDT peer list vs. separate document**: The peer list could be a separate Yrs document instead of embedded in the festival state doc. This would allow different sync priorities (peer list = CRITICAL, alongside festival state). However, it adds complexity. Start with embedding in the festival state doc; extract later if needed.

6. **Festival-scoped zero-knowledge handshake**: The current `group_sync.rs` handshake discovers shared groups. For BLE path 1 (fully offline), we also need to discover shared festivals. Extend the handshake to include festival IDs (which are public, so no zero-knowledge needed — just exchange subscribed festival IDs directly).

## Open Questions

1. **Peer sampling strategy at scale**: At 10,000+ CRDT entries, which internet peers should the connection manager actively connect to? Options: random sampling, topic co-subscribers (requires metadata), geographic proximity (requires location data we don't have).

2. **BLE EndpointId characteristic security**: Exposing the full 32-byte pubkey over BLE to anyone in range. The pubkey is already ~38% exposed via the 12-byte KeyPrefix in advertising. The remaining 20 bytes don't enable impersonation (attacker would need the private key). Acceptable risk?

3. **DO store-and-forward retention**: How long should the DO buffer gossip messages for late joiners? Options: last N messages per topic, last T seconds, or full CRDT state vector (already handled by SV exchange).

4. **Heartbeat when backgrounded**: iOS and Android aggressively suspend background apps. Should the heartbeat use platform-specific background task APIs (BGTaskScheduler on iOS, WorkManager on Android), or only run while foregrounded?

---

## Atomic Task List

### Phase 1: Persist EndpointId
- [ ] Add `secret_key` column to SQLite schema
- [ ] Load or generate SecretKey in `OffbeatNode::new_with_networking()`
- [ ] Write test: create, close, reopen → same EndpointId

### Phase 2: CRDT Peer List — Server
- [ ] Add `POST /festivals/{id}/checkin` route to Festival DO
- [ ] Validate WebAuthn token on checkin
- [ ] Write peer entry to Yrs doc under `peers/`
- [ ] Add pruning logic (remove entries > 2h old)
- [ ] Add periodic pruning alarm (every 15 min)
- [ ] Write server test for checkin + pruning

### Phase 3: CRDT Peer List — Client
- [ ] Add `PeerInfo` struct to `types.rs`
- [ ] Add `parse_peer_list()` to `DocManager`
- [ ] Filter out own EndpointId
- [ ] Handle malformed entries gracefully
- [ ] Write unit test for parse round-trip

### Phase 4: BLE EndpointId Characteristic
- [ ] Define `IROH_ENDPOINT_ID_CHAR_UUID` constant
- [ ] Add characteristic to `build_gatt_services()`
- [ ] Serve 32-byte pubkey on READ requests
- [ ] Add `read_endpoint_id()` method to `BleTransport`
- [ ] Write test with mock BLE interface

### Phase 5: Connection Manager — Core
- [ ] Create `connection_manager.rs` module
- [ ] Implement `peer_discovery_tick` (BLE prefix → CRDT match → join_peers)
- [ ] Implement `reconnect_tick` (re-join dropped peers, scan-hint gated)
- [ ] Implement `heartbeat_tick` (POST to DO)
- [ ] Implement BLE GATT read path (unmatched peers → read EndpointId → ZK handshake)
- [ ] Add internet peer path (CRDT relay URL → add_node_addr)
- [ ] Emit peer status change events
- [ ] Wire into `OffbeatNode`
- [ ] Write unit tests for each tick

### Phase 6: FRB Bridge + Flutter Integration
- [ ] Add FRB-annotated wrappers for connection manager status
- [ ] Add heartbeat trigger on app foreground
- [ ] Expose peer list to Flutter UI (active peers count, connection status)
- [ ] Regenerate FRB bindings

### Phase 7: Gossip Protocol Bridge
- [ ] Define DO's deterministic EndpointId derivation
- [ ] Refactor Festival DO to send/receive `GossipEnvelope` protobuf over WS
- [ ] Implement `WsGossipTransport` on client (WebSocket as gossip peer)
- [ ] Add store-and-forward buffer in DO
- [ ] Remove `PeerConnection` trait, `WsRelaySink`, custom WS sync
- [ ] Migrate `SyncOrchestrator` to use gossip-only path
- [ ] Write integration test: client ↔ DO via gossip protocol

## Success Metrics

| Metric | Target | How to measure |
|--------|--------|----------------|
| Direct peer connections | >0 active gossip peers without WS relay | `gossip.active_peers()` with relay disconnected |
| BLE offline sync | Festival data syncs between two phones in airplane mode | Manual test: two devices, airplane mode, BLE on |
| Peer discovery latency | <30s from BLE discovery to first sync | Timestamp logs: BLE `Discovered` → gossip `NeighborUp` |
| Heartbeat reliability | 95% of foregrounded sessions register within 60 min | Server-side checkin logs |
| CRDT peer list size | Handles 10,000 entries without sync degradation | Load test: populate CRDT with 10k entries, measure sync time |

---

## Artifact 2: JSON LLM-Centric PRD

```json
{
  "project_id": "p2p-direct-connectivity",
  "technical_context": {
    "stack": [
      "Rust (iroh 0.98, iroh-gossip, yrs)",
      "Flutter (Dart, flutter_rust_bridge v2)",
      "Cloudflare Workers + Durable Objects (TypeScript, Hono)",
      "iroh-ble-transport (vendor crate)",
      "SQLite (rusqlite)"
    ],
    "entry_points": [
      "crates/core/src/lib.rs",
      "crates/core/src/gossip_manager.rs",
      "crates/core/src/sync.rs",
      "crates/core/src/group_sync.rs",
      "apps/server/src/festival-do.ts",
      "apps/mobile/rust/src/api/mod.rs",
      "vendor/iroh-ble-transport/crates/iroh-ble-transport/src/transport/transport.rs"
    ]
  },
  "phases": [
    {
      "id": "phase_1",
      "task_name": "Persist EndpointId across restarts",
      "files_impacted": [
        "crates/core/src/lib.rs",
        "crates/core/src/db/mod.rs"
      ],
      "definition_of_done": "cargo test secret_key_persistence && pnpm check",
      "dependencies": []
    },
    {
      "id": "phase_2",
      "task_name": "CRDT Peer List — Server",
      "files_impacted": [
        "apps/server/src/festival-do.ts"
      ],
      "definition_of_done": "pnpm -F @offbeat/server test && pnpm check",
      "dependencies": []
    },
    {
      "id": "phase_3",
      "task_name": "CRDT Peer List — Client parsing",
      "files_impacted": [
        "crates/core/src/doc_manager.rs",
        "crates/core/src/types.rs"
      ],
      "definition_of_done": "cargo test peer_list && pnpm check",
      "dependencies": ["phase_2"]
    },
    {
      "id": "phase_4",
      "task_name": "BLE EndpointId GATT characteristic",
      "files_impacted": [
        "vendor/iroh-ble-transport/crates/iroh-ble-transport/src/transport/transport.rs",
        "vendor/iroh-ble-transport/crates/iroh-ble-transport/src/transport/events.rs"
      ],
      "definition_of_done": "cargo test -p iroh-ble-transport endpoint_id && pnpm check",
      "dependencies": []
    },
    {
      "id": "phase_5",
      "task_name": "Connection Manager",
      "files_impacted": [
        "crates/core/src/connection_manager.rs",
        "crates/core/src/lib.rs",
        "crates/core/src/gossip_manager.rs"
      ],
      "definition_of_done": "cargo test connection_manager && pnpm check",
      "dependencies": ["phase_1", "phase_3", "phase_4"]
    },
    {
      "id": "phase_6",
      "task_name": "FRB Bridge + Flutter integration",
      "files_impacted": [
        "apps/mobile/rust/src/api/mod.rs",
        "apps/mobile/lib/screens/social/social_screen.dart"
      ],
      "definition_of_done": "pnpm check",
      "dependencies": ["phase_5"]
    },
    {
      "id": "phase_7",
      "task_name": "Gossip Protocol Bridge (WS relay unification)",
      "files_impacted": [
        "apps/server/src/festival-do.ts",
        "crates/core/src/ws_relay.rs",
        "crates/core/src/sync.rs",
        "crates/core/src/gossip_manager.rs"
      ],
      "definition_of_done": "pnpm check",
      "dependencies": ["phase_5"]
    }
  ]
}
```

## Artifact 3: Execution Plan

### Phase 1 — Persist EndpointId

**Goal:** Store the iroh SecretKey in SQLite so the EndpointId survives app restarts. This is a prerequisite — without it, every restart invalidates the CRDT peer list entry.

**Dependencies:** None

**Definition of done:** `cargo test --workspace && pnpm check`

**Files:**
- `crates/core/src/db/mod.rs` — add `secret_key` table/column
- `crates/core/src/lib.rs` — load or generate in `new_with_networking()`

**Sub-Agent Prompt:**
```
You are implementing Phase 1 of the P2P Direct Connectivity PRD for the Offbeat project.

CONTEXT: Offbeat is a festival timeline tracker. The Rust core (`crates/core`) uses iroh for P2P networking. Currently, `OffbeatNode::new_with_networking()` in `crates/core/src/lib.rs` generates a fresh `iroh::SecretKey` on every launch (line 154: `let secret_key = iroh::SecretKey::generate()`). This means the node's EndpointId changes every restart, which breaks peer discovery.

TASK: Persist the iroh SecretKey to the SQLite database.

1. Create branch: `git spice branch create phase-1-persist-endpoint-id`
2. In `crates/core/src/db/mod.rs` (or appropriate db module), add a table or key-value entry to store a 32-byte secret key blob.
3. In `crates/core/src/lib.rs`, modify `new_with_networking()`:
   - Try to load an existing SecretKey from the database
   - If none exists, generate a new one and persist it
   - Use the loaded/generated key for the iroh Endpoint
4. Write a test `secret_key_persistence` that:
   - Creates an OffbeatNode with a temp db path
   - Records the EndpointId
   - Drops the node
   - Creates a new OffbeatNode with the same db path
   - Asserts the EndpointId is identical
5. Run `pnpm check` and fix any failures.
6. Commit: `git spice commit -m "persist iroh secret key across restarts"`
7. Run `pnpm turbo check --ui stream --output-logs errors-only`
8. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```

---

### Phase 2 — CRDT Peer List (Server)

**Goal:** Add a checkin endpoint to the Festival DO that writes peer entries to the CRDT with TTL-based pruning.

**Dependencies:** None (parallel with Phase 1)

**Definition of done:** `pnpm -F @offbeat/server test && pnpm check`

**Files:**
- `apps/server/src/festival-do.ts` — add route + CRDT write + pruning

**Sub-Agent Prompt:**
```
You are implementing Phase 2 of the P2P Direct Connectivity PRD for the Offbeat project.

CONTEXT: Offbeat uses Cloudflare Durable Objects for festival state. The Festival DO (`apps/server/src/festival-do.ts`) manages a Yrs CRDT document with stages, days, sets, and weather data. It signs updates with Ed25519 and broadcasts via WebSocket.

TASK: Add a peer checkin endpoint that stores EndpointIds in the CRDT.

1. Create branch: `git spice branch create phase-2-crdt-peer-list-server`
2. In `apps/server/src/festival-do.ts`, add route:
   `POST /festivals/{festival_id}/checkin`
   - Request body: `{ "endpoint_id": string (64-char hex), "relay_url": string | null }`
   - Validate the request (endpoint_id is 64 hex chars)
   - WebAuthn auth validation (use existing auth patterns in the codebase)
   - Write to the Yrs doc: set `peers/{endpoint_id}` to `{ relay_url, last_seen: Date.now()/1000, user_id }`
   - Prune entries where `last_seen` < now - 7200 (2 hours)
   - Sign and broadcast the CRDT update (follow existing patterns)
   - Return `{ ttl: 7200, peer_count: <number of active peers> }`
3. Add a DO alarm (every 15 minutes) that runs the same pruning logic.
4. Write tests for:
   - Successful checkin returns 200 with ttl and peer_count
   - Peer entry appears in CRDT
   - Expired entries are pruned
5. Run `pnpm check` and fix any failures.
6. Commit: `git spice commit -m "add peer checkin endpoint to Festival DO"`
7. Run `pnpm turbo check --ui stream --output-logs errors-only`
8. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```

---

### Phase 3 — CRDT Peer List (Client)

**Goal:** Parse peer list entries from the festival CRDT document on the client side.

**Dependencies:** Phase 2 (peer entries must exist in CRDT to parse)

**Definition of done:** `cargo test --workspace && pnpm check`

**Files:**
- `crates/core/src/doc_manager.rs` — add `parse_peer_list()`
- `crates/core/src/types.rs` — add `PeerInfo` struct

**Sub-Agent Prompt:**
```
You are implementing Phase 3 of the P2P Direct Connectivity PRD for the Offbeat project.

CONTEXT: Offbeat's Rust core parses festival CRDT documents in `crates/core/src/doc_manager.rs`. The Yrs document contains stages, days, sets under string keys. Phase 2 added `peers/{endpoint_id}` entries to this document on the server side. Now the client needs to parse them.

TASK: Add client-side parsing of the CRDT peer list.

1. Create branch stacked on phase-2: `git spice branch create phase-3-crdt-peer-list-client`
2. In `crates/core/src/types.rs`, add:
   ```rust
   pub struct PeerInfo {
       pub endpoint_id: iroh::EndpointId,
       pub relay_url: Option<String>,
       pub last_seen: u64,
       pub user_id: String,
   }
   ```
3. In `crates/core/src/doc_manager.rs`, add a method:
   ```rust
   pub fn parse_peer_list(&self, doc_id: &str, own_endpoint_id: &EndpointId) -> Vec<PeerInfo>
   ```
   - Read all entries under the `peers` key from the Yrs document
   - Parse each entry's JSON value into PeerInfo
   - Filter out `own_endpoint_id`
   - Skip malformed entries (log warning, don't panic)
   - Return the parsed list
4. Write tests:
   - `peer_list_parse`: create a Yrs doc, insert peer entries, parse them back
   - `peer_list_filters_self`: verify own EndpointId is excluded
   - `peer_list_handles_malformed`: insert a malformed entry, verify it's skipped
5. Run `pnpm check` and fix any failures.
6. Commit: `git spice commit -m "parse CRDT peer list on client"`
7. Run `pnpm turbo check --ui stream --output-logs errors-only`
8. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```

---

### Phase 4 — BLE EndpointId GATT Characteristic

**Goal:** Add a READ characteristic to the BLE transport that serves the full 32-byte EndpointId, enabling fully offline peer identity exchange.

**Dependencies:** None (parallel with Phases 1–3)

**Definition of done:** `cargo test -p iroh-ble-transport && pnpm check`

**Files:**
- `vendor/iroh-ble-transport/crates/iroh-ble-transport/src/transport/transport.rs` — add characteristic
- `vendor/iroh-ble-transport/crates/iroh-ble-transport/src/transport/events.rs` — handle read requests

**Sub-Agent Prompt:**
```
You are implementing Phase 4 of the P2P Direct Connectivity PRD for the Offbeat project.

CONTEXT: The iroh-ble-transport crate (in `vendor/iroh-ble-transport/`) provides BLE as a custom transport for iroh. Each node advertises a GATT service UUID encoding a 12-byte KeyPrefix (first 12 bytes of the Ed25519 public key). This is NOT enough for gossip — gossip needs the full 32-byte EndpointId. Read `vendor/iroh-ble-transport/CLAUDE.md` for full architecture details.

TASK: Add a GATT characteristic that serves the full EndpointId.

1. Create branch: `git spice branch create phase-4-ble-endpoint-id-char`
2. In `transport.rs`, define:
   ```rust
   pub const IROH_ENDPOINT_ID_CHAR_UUID: Uuid = uuid_from_static("69726f06-8e45-4c2c-b3a5-331f3098b5c2");
   ```
3. In `build_gatt_services()`, add the characteristic:
   - UUID: IROH_ENDPOINT_ID_CHAR_UUID
   - Properties: READ
   - Value: 32-byte Ed25519 public key (the node's EndpointId as bytes)
4. Handle READ requests in `run_peripheral_events` for this characteristic (follow the PSM characteristic pattern).
5. Add a `read_endpoint_id(&self, device_id: &DeviceId) -> Option<EndpointId>` method to `BleTransport` that reads this characteristic from a connected peer.
6. Write tests using the `testing` feature / mock BLE interface:
   - Characteristic is present in the GATT service
   - Value matches the node's public key
   - `read_endpoint_id` returns correct EndpointId
7. Run `pnpm check` and fix any failures (note: the vendor crate uses `mise run verify` for its own checks).
8. Commit: `git spice commit -m "add EndpointId GATT characteristic to BLE transport"`
9. Run `pnpm turbo check --ui stream --output-logs errors-only`
10. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```

---

### Phase 5 — Connection Manager

**Goal:** Build the central component that bridges BLE discovery, CRDT peer list, and iroh-gossip — implementing all four bootstrap paths.

**Dependencies:** Phase 1, Phase 3, Phase 4

**Definition of done:** `cargo test --workspace && pnpm check`

**Files:**
- `crates/core/src/connection_manager.rs` (new)
- `crates/core/src/lib.rs` — wire into OffbeatNode

**Sub-Agent Prompt:**
```
You are implementing Phase 5 of the P2P Direct Connectivity PRD for the Offbeat project.

CONTEXT: Offbeat's Rust core (`crates/core/src/`) has:
- `lib.rs`: OffbeatNode with iroh Endpoint, Gossip, BleTransport, ResourceRegistry
- `gossip_manager.rs`: GossipManager with subscribe(topic, bootstrap_peers) and join_peers
- `group_sync.rs`: Zero-knowledge group handshake for discovering shared groups between peers
- `resource.rs`: ResourceRegistry tracking active topics (festival state, group state, chat)
- `doc_manager.rs`: DocManager with parse_peer_list() (added in Phase 3)

The BLE transport (Phase 4) now has a GATT characteristic serving the full EndpointId.
The SecretKey is persisted (Phase 1), so EndpointId is stable across restarts.
The CRDT contains peer entries (Phase 2+3).

TASK: Implement the ConnectionManager that ties everything together.

1. Create branch stacked on phase-3 and phase-4: `git spice branch create phase-5-connection-manager`
2. Create `crates/core/src/connection_manager.rs` with:

   ```rust
   pub struct ConnectionManager { ... }
   pub struct ConnectionManagerHandle { ... } // for stopping ticks

   impl ConnectionManager {
       pub fn new(
           endpoint: iroh::Endpoint,
           gossip: Gossip,
           ble_transport: Option<Arc<BleTransport>>,
           resource_registry: Arc<RwLock<ResourceRegistry>>,
           doc_manager: Arc<DocManager>,
       ) -> Self;

       pub async fn start(&self) -> ConnectionManagerHandle;
       pub fn on_peer_list_updated(&self, peers: Vec<PeerInfo>);
       pub fn peer_snapshot(&self) -> Vec<PeerEntry>;
   }
   ```

3. Implement three background ticks (spawned by `start()`):

   a) `peer_discovery_tick` (10s interval):
      - Get BLE snapshot from ble_transport.snapshot_peers()
      - Get CRDT peer list from doc_manager.parse_peer_list()
      - For each BLE-discovered peer with a KeyPrefix:
        - Check if any CRDT peer's EndpointId starts with that prefix
        - If match: endpoint.add_node_addr(NodeAddr with BLE + relay_url), gossip.join_peers([id])
        - If no match: read_endpoint_id() via BLE GATT, then run group handshake to find shared topics
      - For CRDT peers NOT visible via BLE (internet-only):
        - endpoint.add_node_addr(NodeAddr with relay_url only)
        - gossip.join_peers([id])
      - Cap join_peers calls at 20 peers per tick

   b) `heartbeat_tick` (60 min interval):
      - Build checkin payload with own EndpointId and home relay URL
      - POST to Festival DO checkin endpoint
      - Store response TTL for adaptive scheduling

   c) `reconnect_tick` (10s interval):
      - For each known peer not currently in Direct gossip status:
        - If BLE available and peer has scan hint: join_peers
        - If no BLE but peer has relay_url: join_peers
        - Throttle: skip if last attempt was <5s ago

4. Implement peer status tracking:
   - PeerEntry with source (Crdt/Ble/Gossip), gossip_status, ble_prefix_match
   - Emit events on status changes

5. Wire into OffbeatNode in lib.rs:
   - Create ConnectionManager in new_with_networking()
   - Call start() after endpoint is bound
   - Expose peer_snapshot() for FRB

6. Write tests:
   - `connection_manager_ble_match`: mock BLE prefix + CRDT entry → verify join_peers called
   - `connection_manager_internet_peer`: CRDT entry with relay_url, no BLE → verify add_node_addr called
   - `connection_manager_reconnect`: simulate peer drop → verify re-join on next tick
   - `connection_manager_throttle`: verify same peer isn't re-joined within 5s

7. Run `pnpm check` and fix any failures.
8. Commit: `git spice commit -m "implement connection manager with multi-path bootstrap"`
9. Run `pnpm turbo check --ui stream --output-logs errors-only`
10. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```

---

### Phase 6 — FRB Bridge + Flutter Integration

**Goal:** Expose connection manager status to the Flutter UI via FRB bindings.

**Dependencies:** Phase 5

**Definition of done:** `pnpm check`

**Files:**
- `apps/mobile/rust/src/api/mod.rs` — FRB wrappers
- `apps/mobile/lib/screens/social/social_screen.dart` — display peer count

**Sub-Agent Prompt:**
```
You are implementing Phase 6 of the P2P Direct Connectivity PRD for the Offbeat project.

CONTEXT: Offbeat uses flutter_rust_bridge v2 (FRB) to expose Rust APIs to Flutter. The bridge is in `apps/mobile/rust/src/api/mod.rs`. The connection manager (Phase 5) is in `crates/core/src/connection_manager.rs` and tracks peer status.

IMPORTANT: Do NOT edit generated files (frb_generated.rs, frb_generated.dart, etc).

TASK: Add FRB bindings for the connection manager and integrate with Flutter.

1. Create branch stacked on phase-5: `git spice branch create phase-6-frb-integration`
2. In `apps/mobile/rust/src/api/mod.rs`, add FRB-annotated functions:
   - `get_peer_count() -> u32` — returns number of active direct peers
   - `get_peer_list() -> Vec<PeerStatusInfo>` — returns peer snapshots for UI
   - `trigger_heartbeat()` — manually trigger a checkin (for testing)
   Where `PeerStatusInfo` is a simple FRB-compatible struct with endpoint_id (String), source (String), status (String), ble_visible (bool).
3. In `apps/mobile/lib/screens/social/social_screen.dart` (or appropriate screen):
   - Display the direct peer count somewhere visible
   - This can be minimal — just showing "N direct peers" is sufficient for now
4. Run `flutter_rust_bridge_codegen generate` to regenerate bindings.
5. Run `pnpm check` and fix any failures.
6. Commit: `git spice commit -m "expose connection manager status via FRB"`
7. Run `pnpm turbo check --ui stream --output-logs errors-only`
8. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```

---

### Phase 7 — Gossip Protocol Bridge (WS Relay Unification)

**Goal:** Refactor the WS relay to speak the iroh-gossip wire protocol, making the DO "just another gossip peer" and eliminating the dual sync path.

**Dependencies:** Phase 5

**Definition of done:** `pnpm check`

**Files:**
- `apps/server/src/festival-do.ts` — speak gossip wire protocol
- `crates/core/src/ws_relay.rs` — refactor to gossip transport
- `crates/core/src/sync.rs` — remove PeerConnection, use gossip-only
- `crates/core/src/gossip_manager.rs` — accept WS as gossip peer

**Sub-Agent Prompt:**
```
You are implementing Phase 7 of the P2P Direct Connectivity PRD for the Offbeat project. This is the largest phase — a significant refactor of the sync architecture.

CONTEXT: Currently Offbeat has TWO sync paths:
1. WS relay: custom PeerConnection trait, WsRelaySink, SyncOrchestrator — does all actual sync
2. iroh-gossip: GossipManager with subscribe/broadcast — exists but is idle (empty bootstrap)

The goal is to unify these: make the WS relay speak the iroh-gossip wire protocol (protobuf GossipEnvelope), so it appears as a regular gossip peer. The DO can't speak QUIC (CF Workers limitation), so WebSocket is the transport, but the wire format should be standard gossip protocol.

Read these files first:
- crates/core/src/sync.rs — understand PeerConnection trait, SyncOrchestrator
- crates/core/src/ws_relay.rs — understand current WS relay implementation
- crates/core/src/gossip_manager.rs — understand GossipManager
- crates/core/src/proto/mod.rs — understand protobuf message types
- apps/server/src/festival-do.ts — understand DO WebSocket handling

TASK:

1. Create branch stacked on phase-5: `git spice branch create phase-7-gossip-bridge`

2. Server side (apps/server/src/festival-do.ts):
   - Derive a deterministic EndpointId for the DO from the festival signing key
   - Refactor WS message handling to send/receive GossipEnvelope protobuf messages
   - Add store-and-forward: buffer the last 100 messages per topic, replay to new WS connections
   - On WS connect: send buffered messages, then relay live messages

3. Client side (crates/core/src/):
   - Create a WsGossipTransport that wraps the WebSocket connection as a gossip-compatible peer
   - When connecting to the DO, register it as a gossip peer (not a custom PeerConnection)
   - The DO's deterministic EndpointId should be passed as a bootstrap peer to gossip.subscribe()

4. Clean up:
   - Remove the PeerConnection trait from sync.rs (or deprecate)
   - Remove WsRelaySink or refactor it into WsGossipTransport
   - Update SyncOrchestrator to work through gossip only
   - Remove the dual-path logic

5. Ensure backward compatibility during migration:
   - If the DO hasn't been updated yet, the client should fall back to the old protocol
   - Add a version/capability negotiation on WS connect

6. Write tests:
   - Client connects to mock WS server speaking gossip protocol → receives messages
   - Late joiner gets buffered messages
   - Verify PeerConnection trait is no longer used

7. Run `pnpm check` and fix any failures.
8. Commit: `git spice commit -m "unify WS relay with gossip protocol"`
9. Run `pnpm turbo check --ui stream --output-logs errors-only`
10. Report: tasks completed, files changed, tests added and passing, pnpm check status.
```
