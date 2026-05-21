# OFFBEAT — Reactive Resource Layer PRD

> Formalize the data sync lifecycle into a unified abstraction. Replace hand-wired per-entity boilerplate with a trait-driven resource model, reactive streams to Dart, and a clean sync protocol.

## Introduction

Offbeat's core data model has four resource types arranged in a 2×2 matrix:

|  | Public (signed by DO) | Private (encrypted with group key) |
|---|---|---|
| **CRDT Doc** (map-shaped) | Festival state (lineup, stages, sets, announcements) | Group state (members, check-ins, stars, pins) |
| **Append Log** (ordered messages) | Stage chat | Group chat |

Today, each cell is wired up independently — hand-rolled SQL CRUD, custom wire encoding, manual seq tracking, bespoke dispatch matching, and per-entity bridge DTOs. The same lifecycle (topic derivation → subscribe → catch-up → persist → stream to UI) is reimplemented four times with subtle differences.

This PRD formalizes that lifecycle into a **resource abstraction** in `crates/core`, a **unified sync protocol**, **reactive FRB streams** to Dart, and an **iroh custom transport** replacing the WebSocket relay.

## Goals & Objectives

| Goal | Metric | Target |
|------|--------|--------|
| Reduce sync boilerplate | Lines of code in gossip_manager + ws_relay + db CRUD | -60% |
| Unified catch-up protocol | Number of distinct sync mechanisms | 2 (SV exchange + HWM) down from 3 |
| Reactive UI | Dart screens using pull-based refresh | 0 (all stream-based) |
| Transport unification | Separate transport abstractions (ws_relay vs gossip) | 1 (iroh only) |
| Test reduction | Boilerplate test lines (manual SQL assertions) | -50% |

## Target Audience

Same as main PRD — festival-goers and organizers.

## Resource Model

### Resource Trait

Every syncable data type implements a common interface:

```rust
/// The two fundamental data shapes.
enum ResourceKind {
    /// Yrs document — map or array-shaped, CRDT merge semantics.
    CrdtDoc,
    /// Append-only ordered messages — high-water mark sync.
    AppendLog,
}

/// Visibility determines how data is protected on the wire.
enum Visibility {
    /// Signed by festival DO's Ed25519 key. Anyone can verify.
    PublicSigned { public_key: [u8; 32] },
    /// Encrypted with a shared AES-256-GCM group key.
    PrivateEncrypted { group_key: [u8; 32] },
}

/// Sync priority — lower number = synced first on connect.
/// Used by the sync orchestrator to order catch-up requests.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Priority(u8);

impl Priority {
    const CRITICAL: Self = Self(0);  // Blocks UI render
    const HIGH: Self = Self(1);      // Core social features
    const MEDIUM: Self = Self(2);    // Recent chat
    const LOW: Self = Self(3);       // Historical / on-navigate
}

trait Resource {
    /// Unique identifier for this resource instance.
    fn id(&self) -> &str;

    /// CRDT doc or append log.
    fn kind(&self) -> ResourceKind;

    /// How this resource is protected on the wire.
    fn visibility(&self) -> Visibility;

    /// Gossip topic for real-time push.
    fn topic(&self) -> TopicId;

    /// Human-readable topic string (for DO relay protocol).
    fn topic_string(&self) -> String;

    /// Sync priority when connecting to a new peer.
    fn priority(&self) -> Priority;
}
```

### Concrete Resources

| Resource | Kind | Visibility | Priority | Topic derivation |
|----------|------|-----------|----------|-----------------|
| `FestivalState` | CrdtDoc | PublicSigned | CRITICAL (0) | `blake3("offbeat/{fest_id}/state")` |
| `GroupState` | CrdtDoc | PrivateEncrypted | HIGH (1) | `blake3(group_key \|\| "state")` |
| `GroupChat` | AppendLog | PrivateEncrypted | MEDIUM (2) | `blake3(group_key \|\| "chat")` |
| `StageChat` | AppendLog | PublicSigned | LOW (3) | `blake3("offbeat/{fest_id}/chat/{stage_id}")` |

### Resource Registry

The `OffbeatNode` holds a `ResourceRegistry` — a collection of all active resource instances. When the user opens a festival, the registry is populated with one `FestivalState`, N `StageChat` instances, and any joined groups' `GroupState` + `GroupChat` instances.

```rust
struct ResourceRegistry {
    resources: HashMap<String, Box<dyn Resource>>,
}

impl ResourceRegistry {
    /// Return all resources sorted by priority (lowest number first).
    fn by_priority(&self) -> Vec<&dyn Resource>;

    /// Register a resource and trigger sync.
    fn register(&mut self, resource: Box<dyn Resource>);

    /// Deregister a resource (unsubscribe, stop streaming).
    fn deregister(&mut self, id: &str);
}
```

## Sync Protocol

### CRDT Resources — State Vector Exchange

For `CrdtDoc` resources, catch-up uses Yrs state vector exchange:

```
  Client                        Peer (DO or P2P)
  ──────                        ────────────────
  1. subscribe(topic)       ──►  (start receiving live gossip)
  2. send(my_state_vector)  ──►
                            ◄──  3. diff = peer_doc.diff(client_sv)
  4. apply(diff)                 (one blob, compressed)
  5. apply any buffered gossip   (Yrs idempotent — dupes are no-ops)
```

**Wire protocol addition** — new message types for DO WebSocket:

```json
// Client → DO
{ "type": "sv_exchange", "docId": "festival/fest-1/state", "sv": "<base64>" }

// DO → Client
{ "type": "sv_diff", "docId": "festival/fest-1/state", "diff": "<base64>" }
```

The DO must hold the current Yrs doc state (not just a gossip log) so it can compute diffs. For encrypted resources, the SV and diff are encrypted with the group key before transmission.

**What this replaces:**
- `gossip_log` replay for CRDT data
- `topic_sync_state` table (client-side)
- `get_server_seq` / `set_server_seq` in `db/mod.rs`
- `sinceSeq`-based catchup in `ws_relay.rs` (for CRDT resources)

### Chat Resources — High-Water Mark Sync

For `AppendLog` resources, catch-up uses a per-writer high-water mark:

```rust
/// Compact representation of "what chat messages I have."
/// One entry per unique writer, not per message.
type ChatStateVector = HashMap<String, u64>;  // user_id → max_seq
```

Each chat message carries a `(user_id, writer_seq)` pair. The writer bumps their counter locally on each send. The sync protocol:

```
  Client                        Peer (DO or P2P)
  ──────                        ────────────────
  1. subscribe(topic)       ──►  (start receiving live gossip)
  2. send(my_chat_sv)       ──►  {"alice": 12, "bob": 7}
                            ◄──  3. batch of msgs where
                                    alice.seq > 12 OR
                                    bob.seq > 7 OR
                                    writer NOT IN client_sv
                                    (newest first, paginated)
  4. INSERT OR IGNORE each       (dedup by message ID)
  5. live gossip continues       (new messages, dedup on insert)
```

**Gaps:** A simple high-water mark means if you have alice:1-3 and alice:10-13 but miss 4-9, you'd re-request 10-13. `INSERT OR IGNORE` makes this harmless. Gaps are rare (only if catchup was interrupted) and the waste is minimal.

**Wire protocol addition:**

```json
// Client → DO
{ "type": "chat_catchup", "topic": "festival/fest-1/chat/main-stage", "sv": {"alice": 12, "bob": 7}, "limit": 50 }

// DO → Client
{ "type": "chat_diff", "topic": "...", "messages": [...] }
```

**What this replaces:**
- `sinceSeq`-based catchup for chat
- `gossip_log` sequential scan (server-side, for chat topics)

### Sync Orchestrator

A single component runs the subscribe→catch-up→live flow for all registered resources, in priority order:

```rust
struct SyncOrchestrator {
    registry: Arc<ResourceRegistry>,
}

impl SyncOrchestrator {
    /// Called when a connection to a peer is established.
    /// Processes all registered resources in priority order.
    async fn sync_with_peer(&self, peer: &PeerConnection) {
        let resources = self.registry.by_priority();
        for resource in resources {
            // 1. Subscribe to gossip (buffering starts)
            peer.subscribe(resource.topic()).await;
            // 2. Catch-up based on resource kind
            match resource.kind() {
                ResourceKind::CrdtDoc => {
                    let sv = self.get_state_vector(resource.id());
                    let diff = peer.sv_exchange(resource.id(), sv).await;
                    self.apply_diff(resource.id(), diff);
                }
                ResourceKind::AppendLog => {
                    let chat_sv = self.get_chat_sv(resource.id());
                    let msgs = peer.chat_catchup(resource.id(), chat_sv, 50).await;
                    self.apply_messages(resource.id(), msgs);
                }
            }
            // 3. Apply buffered gossip (idempotent)
            self.flush_buffer(resource.topic());
        }
    }
}
```

### Sync Priority On Connect

When meeting a new peer, resources sync in this order:

| Order | Resource | Why | Expected size |
|-------|----------|-----|---------------|
| 1 | FestivalState | Can't render UI without lineup | ~15KB |
| 2 | GroupState (each) | Core social features: who's where | ~10KB each |
| 3 | GroupChat (each, recent first) | Friends > strangers | ~30KB each |
| 4 | StageChat (current stage first, recent first) | High volume, low urgency | ~300KB each |

## Reactive Streams via FRB

### Stream Architecture

The Rust core exposes typed streams per resource. FRB v2 natively supports `Stream<T>` over FFI.

```rust
// In crates/bridge (FRB-annotated)
impl AppNode {
    /// Watch the festival lineup. Emits current state immediately,
    /// then re-emits on every Yrs update.
    pub fn watch_lineup(&self, festival_id: String) -> Stream<LineupDto>;

    /// Watch group members + check-ins. Emits on every change.
    pub fn watch_group_state(&self, group_id: String) -> Stream<GroupStateDto>;

    /// Watch chat messages for a topic. Emits new messages as they arrive.
    /// Call with `last_n` to get the initial window, then live updates.
    pub fn watch_chat(&self, topic: String, last_n: u32) -> Stream<Vec<ChatMessageDto>>;

    /// Watch the sync status of all resources.
    pub fn watch_sync_status(&self) -> Stream<SyncStatusDto>;
}
```

### Write Commands

Writes are local-first: mutate locally, queue for gossip, return immediately.

```rust
impl AppNode {
    /// Check in to a stage. Updates local CRDT, queues gossip broadcast.
    pub async fn check_in(&self, group_id: String, stage_id: Option<String>) -> Result<()>;

    /// Star/unstar a set. Updates local state, queues gossip broadcast.
    pub async fn toggle_star(&self, festival_id: String, set_id: String) -> Result<bool>;

    /// Send a chat message. Persists locally, queues gossip broadcast.
    pub async fn send_chat(&self, topic: String, text: String) -> Result<ChatMessageDto>;
}
```

The stream for the affected resource emits the updated state immediately after the local mutation — before gossip delivery. The UI feels instant.

### Dart Side

```dart
// In Flutter — every screen just subscribes
class FestivalDetailScreen extends StatelessWidget {
  Widget build(BuildContext context) {
    return StreamBuilder<LineupDto>(
      stream: node.watchLineup(festivalId),
      builder: (ctx, snapshot) => /* render lineup */,
    );
  }
}

class GroupScreen extends StatelessWidget {
  Widget build(BuildContext context) {
    return StreamBuilder<GroupStateDto>(
      stream: node.watchGroupState(groupId),
      builder: (ctx, snapshot) => /* render members, check-ins, pins */,
    );
  }
}
```

## WS Relay → iroh Custom Transport

### Current State

`ws_relay.rs` (350 lines) is a hand-built WebSocket client that:
- Connects to the DO's WebSocket endpoint
- Implements its own subscribe/unsubscribe protocol
- Tracks `last_seen_seq` per topic
- Does its own reconnect with exponential backoff
- Feeds received messages into `dispatch_message`

This is a parallel transport alongside iroh-gossip, with its own protocol, its own state tracking, and its own reconnect logic.

### Target State

Replace `ws_relay.rs` with an iroh custom transport that speaks WebSocket underneath. The DO becomes just another peer in iroh's transport hierarchy:

```
iroh::Endpoint
  ├── QUIC (direct/relay)     ← default
  ├── BLE                      ← proximity
  ├── Meshtastic               ← mesh
  └── WebSocket (custom)       ← DO relay ← NEW
```

The custom transport implements iroh's datagram transport traits (available via `unstable-custom-transports` feature, already enabled in `Cargo.toml`). WebSocket frames carry iroh datagrams. iroh handles multiplexing, gossip routing, and reconnect.

### Benefits

- Delete `ws_relay.rs` entirely (~350 lines)
- Delete `WsRelaySink`, `WsClientMessage`, `WsServerMessage` types
- Delete manual reconnect logic (iroh handles it)
- Delete manual seq tracking (replaced by Yrs SV exchange / chat HWM)
- Gossip is truly transport-agnostic — same code path regardless of how data arrived

### DO Protocol Changes

The DO's WebSocket protocol must change to speak iroh datagrams instead of the current JSON-envelope protocol. This is a server-side change in `festival-do.ts`.

Alternatively (for the "server stays JS" constraint): the custom transport could adapt between iroh datagrams on the client side and the existing JSON protocol on the server side. This is a translation layer in the transport implementation. Less clean, but avoids changing the DO.

## Data Schema Changes

### Client-Side SQLite

**New tables:**

```sql
-- Chat messages gain a writer_seq column for HWM sync
ALTER TABLE chat_messages ADD COLUMN writer_seq INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_chat_writer_seq ON chat_messages(topic, user_id, writer_seq);
```

**Deleted tables:**

```sql
DROP TABLE gossip_log;        -- Replaced by Yrs SV exchange (CRDTs) and chat HWM
DROP TABLE topic_sync_state;  -- No more server seq tracking
```

### Server-Side DO SQLite

The DO adds Yrs doc storage for state vector exchange:

```sql
CREATE TABLE IF NOT EXISTS yrs_docs (
    doc_id TEXT PRIMARY KEY,
    data BLOB NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

The `gossip_log` table remains for chat catchup but is no longer used for CRDT state catch-up.

## Files Impacted

### Core Changes (`crates/core/src/`)

| File | Change |
|------|--------|
| `resource.rs` | **NEW** — Resource trait, ResourceKind, Visibility, Priority, concrete impls |
| `sync.rs` | **NEW** — SyncOrchestrator, catch-up protocol implementations |
| `gossip_manager.rs` | **SIMPLIFY** — remove wire encoding/decoding, GossipMessage enum, dispatch. Becomes thin: subscribe, broadcast Yrs diffs or chat messages |
| `ws_relay.rs` | **DELETE** — replaced by iroh custom transport |
| `ws_transport.rs` | **NEW** — iroh custom transport over WebSocket |
| `db/mod.rs` | **SIMPLIFY** — remove gossip_log CRUD, topic_sync_state CRUD, generalize doc/chat persistence |
| `db/schema.sql` | **MODIFY** — drop gossip_log + topic_sync_state, add writer_seq to chat_messages |
| `doc_manager.rs` | **MODIFY** — integrate with Resource trait for persistence callbacks |
| `chat.rs` | **MODIFY** — add writer_seq, ChatStateVector, HWM sync methods |
| `groups.rs` | **SIMPLIFY** — remove manual encrypt-then-broadcast pattern (Resource handles it) |
| `types.rs` | **SIMPLIFY** — remove wire-format types (GossipWireMessage, SignedUpdate move to resource layer) |
| `lib.rs` | **MODIFY** — OffbeatNode gains ResourceRegistry + SyncOrchestrator |

### Bridge Changes (`apps/mobile/rust/src/api/`)

| File | Change |
|------|--------|
| `mod.rs` | **REWRITE** — replace pull-based methods with `Stream<T>` per resource. Delete per-entity DTOs that duplicate core types. Delete manual gossip broadcast code. |

### Server Changes (`apps/server/src/`)

| File | Change |
|------|--------|
| `festival-do.ts` | **MODIFY** — add `yrs_docs` table, support `sv_exchange` and `chat_catchup` WS message types. Keep existing `gossip_log` for chat only. |

### Flutter Changes (`apps/mobile/lib/`)

| File | Change |
|------|--------|
| `screens/**` | **MODIFY** — replace manual data fetching with StreamBuilder on FRB streams |
| `services/**` | **SIMPLIFY** — services become thin wrappers around stream subscriptions |

## Non-Functional Requirements

| Requirement | Target |
|-------------|--------|
| Catch-up latency (CRDT, 3G) | < 200ms for festival state |
| Catch-up latency (chat, 50 msgs) | < 500ms |
| Stream update latency (local write → UI) | < 16ms (one frame) |
| Memory overhead per stream | < 1KB (channel + callback) |
| Yrs doc memory (festival state) | < 100KB |
| Chat SQLite query (last 50) | < 5ms |

## Success Metrics

1. `gossip_manager.rs` reduced from ~720 lines to < 100
2. `ws_relay.rs` deleted (350 lines)
3. `db/mod.rs` reduced from ~620 lines to < 300
4. Bridge `mod.rs` reduced from ~840 lines to < 400
5. Zero pull-based data fetching in Dart (all streams)
6. `pnpm check` passes, `cargo test --workspace` passes
7. Two-device sync test: check-in appears on other device in < 1s

## Open Questions

1. **iroh custom transport stability** — The `unstable-custom-transports` API may change. How tightly should we couple to it vs. using an adapter?
2. **DO protocol migration** — Should the DO speak iroh datagrams natively (cleaner) or should the custom transport translate between iroh and the existing JSON protocol (less server churn)?
3. **Chat writer_seq persistence** — Should each client track its own monotonic counter in SQLite, or derive it from a hash of the message content?
4. **Stream backpressure** — If the Dart side is slow to consume, should streams drop intermediate states (latest-value-wins for CRDTs) or buffer (for chat)?
5. **Yrs doc size limits** — Should we compact/snapshot Yrs docs periodically to prevent unbounded growth?

## Future Considerations

- Port the CF DO to Rust (via `workers-rs` or a standalone relay binary)
- BLE transport via iroh custom transports (reference impl exists)
- Meshtastic transport via iroh custom transports
- Conflict-free chat reactions (could be a CRDT map overlay on the chat log)
