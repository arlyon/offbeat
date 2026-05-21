# OFFBEAT — Reactive Resource Layer Execution Plan

8 phases, executed sequentially via isolated sub-agents. Each phase runs in a git worktree, creates a stacked branch, and must pass `pnpm check` before completion.

---

## Phase 1: Resource Trait and Registry

**Goal:** Define the foundational `Resource` trait, `ResourceKind`, `Visibility`, `Priority`, and concrete implementations for all four resource types. Create `ResourceRegistry`.

**Dependencies:** None

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 1 of the Reactive Resource Layer for the Offbeat festival app.

## Context

Offbeat is a Flutter + Rust festival companion app. The Rust core is in `crates/core/src/`. It currently has hand-wired sync logic for four resource types: festival state (CRDT), group state (CRDT), stage chat (append log), group chat (append log). Each has its own topic derivation, visibility model, and sync protocol — all manually coded.

You are building the foundational abstraction that will unify these.

## Task

Create `crates/core/src/resource.rs` with:

1. **`ResourceKind` enum** — `CrdtDoc` and `AppendLog` variants.

2. **`Visibility` enum** — `PublicSigned { public_key: [u8; 32] }` and `PrivateEncrypted { group_key: [u8; 32] }`.

3. **`Priority` struct** — `Priority(u8)` with associated constants:
   - `CRITICAL = 0` (blocks UI render)
   - `HIGH = 1` (core social features)
   - `MEDIUM = 2` (recent chat)
   - `LOW = 3` (historical / on-navigate)
   Implement `PartialOrd`, `Ord`, `PartialEq`, `Eq`, `Clone`, `Copy`.

4. **`Resource` trait** with methods:
   - `fn id(&self) -> &str`
   - `fn kind(&self) -> ResourceKind`
   - `fn visibility(&self) -> Visibility`
   - `fn topic(&self) -> iroh_gossip::proto::TopicId`
   - `fn topic_string(&self) -> String`
   - `fn priority(&self) -> Priority`

5. **Concrete implementations:**
   - `FestivalState { festival_id: String, public_key: [u8; 32] }` — CrdtDoc, PublicSigned, CRITICAL
   - `GroupState { group_id: String, group_key: [u8; 32] }` — CrdtDoc, PrivateEncrypted, HIGH
   - `GroupChat { group_id: String, group_key: [u8; 32] }` — AppendLog, PrivateEncrypted, MEDIUM
   - `StageChat { festival_id: String, stage_id: String }` — AppendLog, PublicSigned (but signing is optional for chat — use a sentinel key or a separate variant), LOW

   Each impl should use the existing topic derivation from `crates/core/src/topics.rs`:
   - `FestivalState` → `topics::festival_topic(festival_id, "state")`
   - `GroupState` → `topics::group_topic(group_key, "state")`
   - `GroupChat` → `topics::group_topic(group_key, "chat")`
   - `StageChat` → `topics::festival_topic(festival_id, &format!("chat/{stage_id}"))`

6. **`ResourceRegistry`** struct:
   - `resources: HashMap<String, Box<dyn Resource + Send + Sync>>`
   - `fn register(&mut self, resource: Box<dyn Resource + Send + Sync>)`
   - `fn deregister(&mut self, id: &str)`
   - `fn get(&self, id: &str) -> Option<&dyn Resource>`
   - `fn by_priority(&self) -> Vec<&dyn Resource>` — returns all resources sorted by priority (ascending)

7. **Add `pub mod resource;`** to `crates/core/src/lib.rs`.

8. **Tests** in `resource.rs`:
   - Test that each concrete resource returns the correct kind, visibility, and priority
   - Test that topic derivation matches the existing functions in `topics.rs`
   - Test that `ResourceRegistry::by_priority` returns resources in correct order
   - Test register and deregister

## Important

- Do NOT modify any existing files except adding `pub mod resource;` to `lib.rs`
- The existing code must continue to work unchanged — this phase is purely additive
- Use the existing `topics` module for topic derivation — do not duplicate logic
- Run `cargo clippy --workspace --all-targets -- -D warnings` and fix any warnings
- Run `cargo test --workspace` and ensure all tests pass

## Branch

Create a stacked branch: `git spice branch create phase-1-resource-trait`

## Commit

Commit with: `git spice commit create -m "add Resource trait, concrete impls, and ResourceRegistry"`

## Validate

Run `pnpm turbo check --ui stream --output-logs errors-only` and fix any failures.

## Report

When done, report: files created/changed, tests added and whether they pass, and whether `pnpm check` passed or failed.
```

---

## Phase 2: CRDT Sync Protocol — State Vector Exchange

**Goal:** Replace gossip_log-based catchup for CRDT resources with Yrs state vector exchange. The DO holds Yrs docs and computes diffs. Client sends its state vector, gets one diff blob.

**Dependencies:** Phase 1

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 2 of the Reactive Resource Layer for the Offbeat festival app.

## Context

Offbeat is a Flutter + Rust festival companion. Phase 1 added a Resource trait in `crates/core/src/resource.rs`. Now we're replacing the gossip_log-based catchup for CRDT resources (FestivalState, GroupState) with Yrs state vector exchange.

Currently:
- Client connects to DO WebSocket, sends `{ type: "catchup", topic, sinceSeq }`.
- DO queries its `gossip_log` table and replays all entries since that seq.
- Client applies each one individually via `dispatch_message`.

Target:
- Client sends its Yrs state vector for a doc.
- DO (or any peer) computes a diff and returns it.
- Client applies ONE diff. Done.

## Task

### Server side (`apps/server/src/festival-do.ts`)

1. Add a `yrs_docs` table to the DO's schema:
   ```sql
   CREATE TABLE IF NOT EXISTS yrs_docs (
       doc_id TEXT PRIMARY KEY,
       data BLOB NOT NULL,
       updated_at TEXT NOT NULL DEFAULT (datetime('now'))
   );
   ```

2. When the DO processes a `festival_update` gossip message (in `webSocketMessage` and `seedLineup`), also apply the Yrs update to the stored doc in `yrs_docs`. Use the `yjs` library already imported to:
   - Load the existing doc (or create a new one)
   - Apply the update
   - Store the full encoded state back

3. Add a new WS message type `sv_exchange`:
   - Client sends: `{ type: "sv_exchange", docId: "festival/fest-1/state", sv: "<base64>" }`
   - DO loads its Yrs doc for that docId, computes `Y.encodeStateAsUpdate(doc, decodedSV)`, responds: `{ type: "sv_diff", docId: "...", diff: "<base64>" }`

4. For encrypted (group) docs, the SV and diff travel encrypted. The DO cannot decrypt them, so for group CRDT docs the DO stores the encrypted Yrs doc as an opaque blob and cannot compute diffs. Group SV exchange happens peer-to-peer only (already implemented in `groups.rs`). The DO path is only for festival (public) docs.

### Client side (`crates/core/src/`)

5. Add a `sv_exchange` method to `WsRelaySink` in `ws_relay.rs`:
   ```rust
   pub async fn sv_exchange(&self, doc_id: &str, sv_bytes: Vec<u8>) -> anyhow::Result<()>
   ```
   This sends `{ type: "sv_exchange", docId, sv: base64(sv_bytes) }` to the DO.

6. Handle the `sv_diff` response in `ws_relay.rs`'s `WsServerMessage` enum and `handle_server_message`:
   - Decode the base64 diff
   - Apply it to the local Yrs doc via `doc_manager.apply_update(doc_id, &diff_bytes)`

7. Modify `subscribe_festival` in the bridge (`apps/mobile/rust/src/api/mod.rs`):
   - Instead of `request_catchup(topic, since_seq)`, do:
     1. Subscribe to the topic (existing)
     2. Get the local state vector: `doc_manager.get_state_vector(doc_id)` (or empty SV if doc doesn't exist)
     3. Call `ws.sv_exchange(doc_id, sv_bytes)`
   - Remove the `get_server_seq` call

8. **Do NOT delete** `topic_sync_state` or `gossip_log` tables yet — chat still uses them. Just stop using them for CRDT resources.

### Tests

9. Add a test in `ws_relay.rs` that verifies the `sv_exchange` WS message serializes correctly.

10. Add a test (can be in `doc_manager.rs` or a new integration test) that simulates the SV exchange flow:
    - Create two DocManagers (D1 and D2)
    - D1 makes changes to a doc
    - D2 sends its SV to D1
    - D1 computes diff
    - D2 applies diff
    - Verify D2 has D1's changes

## Important

- Read the existing files before modifying them: `ws_relay.rs`, `festival-do.ts`, `apps/mobile/rust/src/api/mod.rs`
- The existing catchup flow must still work for chat — only CRDT resources change
- Run `pnpm turbo check --ui stream --output-logs errors-only` and fix any failures

## Branch

Create a stacked branch on top of phase-1: `git spice branch create phase-2-sv-exchange`

## Commit

Commit with: `git spice commit create -m "replace gossip_log catchup with state vector exchange for CRDT resources"`

## Validate

Run `pnpm turbo check --ui stream --output-logs errors-only` and fix any failures.

## Report

When done, report: files changed, tests added and whether they pass, and whether `pnpm check` passed or failed.
```

---

## Phase 3: Chat Sync Protocol — High-Water Mark

**Goal:** Add `writer_seq` to chat messages. Implement `ChatStateVector`-based catchup. Replace `sinceSeq`-based catchup for chat.

**Dependencies:** Phase 2

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 3 of the Reactive Resource Layer for the Offbeat festival app.

## Context

Phase 2 replaced CRDT catchup with state vector exchange. Chat still uses the old sinceSeq-based catchup. Now we're replacing that with a per-writer high-water mark protocol.

## Task

### Schema changes

1. In `crates/core/src/db/schema.sql`, add `writer_seq` to `chat_messages`:
   ```sql
   -- Add to CREATE TABLE chat_messages:
   writer_seq INTEGER NOT NULL DEFAULT 0
   ```
   Add index: `CREATE INDEX IF NOT EXISTS idx_chat_writer_seq ON chat_messages(topic, user_id, writer_seq);`

2. In `crates/core/src/types.rs`, add `writer_seq: u64` to `ChatMessage`.

### Chat module changes (`crates/core/src/chat.rs`)

3. Add a `ChatStateVector` type alias:
   ```rust
   pub type ChatStateVector = HashMap<String, u64>;  // user_id → max writer_seq
   ```

4. Add `compute_chat_sv(topic: &str) -> ChatStateVector` to `ChatManager`:
   - Query: `SELECT user_id, MAX(writer_seq) FROM chat_messages WHERE topic = ? GROUP BY user_id`
   - Return the HashMap

5. In `send_festival_chat` and `send_group_chat`, assign `writer_seq` by querying the current max for the user+topic and incrementing:
   - `SELECT COALESCE(MAX(writer_seq), 0) FROM chat_messages WHERE topic = ? AND user_id = ?` + 1

6. Add `get_messages_since_sv(topic: &str, sv: &ChatStateVector, limit: u32) -> Vec<ChatMessage>`:
   - For each writer NOT in the SV, include all their messages
   - For each writer IN the SV, include messages with writer_seq > sv[writer]
   - Order by timestamp DESC (newest first)
   - Limit total results

### Server changes (`apps/server/src/festival-do.ts`)

7. Add a `writer_seq` column to the DO's gossip_log or chat-specific table.

8. Add a new WS message type `chat_catchup`:
   - Client sends: `{ type: "chat_catchup", topic: "...", sv: { "alice": 12, "bob": 7 }, limit: 50 }`
   - DO computes the diff: messages where writer's seq > sv[writer] or writer not in sv
   - Responds: `{ type: "chat_diff", topic: "...", messages: [...] }` (newest first)

### Client integration

9. In `ws_relay.rs`:
   - Add `chat_catchup` method to `WsRelaySink`
   - Handle `chat_diff` response in `WsServerMessage`
   - Apply received messages via `INSERT OR IGNORE`

10. Remove the `sinceSeq`-based catchup path for chat topics.

11. Delete `topic_sync_state` table from `schema.sql` and remove `get_server_seq`/`set_server_seq` from `db/mod.rs`.

12. Delete `gossip_log` table from client-side `schema.sql` and remove `save_gossip`/`get_gossip_since` from `db/mod.rs`.

### Tests

13. Test `compute_chat_sv` returns correct per-writer max seq.
14. Test `get_messages_since_sv` returns only messages not covered by the SV.
15. Test that duplicate messages (same writer_seq) are handled by INSERT OR IGNORE.
16. Update existing chat tests to include writer_seq field.

## Important

- Read all files before modifying: `chat.rs`, `db/mod.rs`, `schema.sql`, `types.rs`, `ws_relay.rs`, `festival-do.ts`
- Update ALL existing tests that construct ChatMessage to include the new writer_seq field
- Ensure `serde` serialization includes writer_seq (camelCase: writerSeq)

## Branch

`git spice branch create phase-3-chat-hwm`

## Commit

`git spice commit create -m "replace sinceSeq catchup with per-writer high-water mark for chat"`

## Validate

`pnpm turbo check --ui stream --output-logs errors-only`

## Report

Files changed, tests added/updated, pass/fail.
```

---

## Phase 4: Sync Orchestrator

**Goal:** Single component that runs subscribe→catch-up→live for all registered resources in priority order. Simplify `gossip_manager.rs` to just subscribe/broadcast.

**Dependencies:** Phase 3

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 4 of the Reactive Resource Layer for the Offbeat festival app.

## Context

Phases 1-3 added:
- Resource trait + registry (`resource.rs`)
- State vector exchange for CRDT resources
- High-water mark catchup for chat resources
- Deleted gossip_log and topic_sync_state from client

Now we unify the sync flow into a single SyncOrchestrator and simplify gossip_manager.

## Task

### SyncOrchestrator (`crates/core/src/sync.rs`)

1. Create or extend `crates/core/src/sync.rs` with a `SyncOrchestrator` struct:

   ```rust
   pub struct SyncOrchestrator {
       registry: Arc<RwLock<ResourceRegistry>>,
       doc_manager: Arc<Mutex<DocManager>>,
       chat_manager: Arc<ChatManager>,
       db: Arc<Database>,
   }
   ```

2. Implement `sync_with_peer` method:
   - Get all resources sorted by priority from the registry
   - For each resource:
     a. Subscribe to gossip topic (via the peer connection handle)
     b. Based on `resource.kind()`:
        - `CrdtDoc` → get local state vector, send sv_exchange, apply diff
        - `AppendLog` → compute ChatStateVector, send chat_catchup, apply messages
     c. Based on `resource.visibility()`:
        - `PublicSigned` → verify signatures on received data
        - `PrivateEncrypted` → encrypt SV/HWM before sending, decrypt diff on receive
   - Return a summary of what was synced

3. The orchestrator should accept a trait-object "PeerConnection" so it can work with both the WS relay and future iroh transport:
   ```rust
   #[async_trait]
   pub trait PeerConnection: Send + Sync {
       async fn subscribe(&self, topics: Vec<String>) -> anyhow::Result<()>;
       async fn sv_exchange(&self, doc_id: &str, sv: Vec<u8>) -> anyhow::Result<Vec<u8>>;
       async fn chat_catchup(&self, topic: &str, sv: &ChatStateVector, limit: u32) -> anyhow::Result<Vec<ChatMessage>>;
   }
   ```

### Simplify gossip_manager.rs

4. Remove the `GossipMessage` enum (7 variants) — no longer needed. Gossip now carries:
   - Raw Yrs diff bytes (for CRDT resources)
   - Raw chat message JSON (for chat resources, possibly encrypted)

5. Remove `GossipWireMessage` struct, `encode_gossip_message`, `decode_wire_message`, `dispatch_message`.

6. Replace with a simpler model:
   ```rust
   pub struct GossipManager {
       gossip: Gossip,
       subscriptions: HashMap<TopicId, GossipSender>,
   }

   impl GossipManager {
       pub async fn subscribe(&mut self, topic_id: TopicId, bootstrap: Vec<EndpointId>) -> Result<()>;
       pub async fn unsubscribe(&mut self, topic_id: TopicId) -> Result<()>;
       pub async fn broadcast(&mut self, topic_id: TopicId, data: Vec<u8>) -> Result<()>;
   }
   ```

7. Incoming gossip messages are routed to the SyncOrchestrator (or a callback) which uses the ResourceRegistry to determine how to handle them (which resource, decrypt/verify, apply to doc or insert to DB).

### Wire into OffbeatNode

8. Add `SyncOrchestrator` and `ResourceRegistry` to `OffbeatNode` in `lib.rs`.
9. The existing `connect_relay` flow should use the SyncOrchestrator instead of manually subscribing and requesting catchup.

### Tests

10. Test SyncOrchestrator processes resources in priority order.
11. Test that CRDT resources use SV exchange path.
12. Test that AppendLog resources use chat_catchup path.
13. Remove obsolete tests that tested the deleted GossipMessage dispatch.

## Important

- Read `gossip_manager.rs`, `ws_relay.rs`, `lib.rs`, `sync.rs` thoroughly before making changes.
- The gossip receive loop still needs to forward messages to the right handler — use the ResourceRegistry to look up the handler.
- Ensure all existing integration tests still pass.
- Do not touch the bridge layer (Phase 5) or Dart (Phase 6) — those come later.

## Branch

`git spice branch create phase-4-sync-orchestrator`

## Commit

`git spice commit create -m "add SyncOrchestrator, simplify gossip_manager to subscribe/broadcast"`

## Validate

`pnpm turbo check --ui stream --output-logs errors-only`

## Report

Files changed, lines removed, tests added/removed, pass/fail.
```

---

## Phase 5: Reactive FRB Streams

**Goal:** Replace pull-based bridge API with `Stream<T>` per resource. Writes mutate locally and queue for gossip.

**Dependencies:** Phase 4

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 5 of the Reactive Resource Layer for the Offbeat festival app.

## Context

Phases 1-4 built the Resource trait, sync protocols, and SyncOrchestrator in `crates/core`. The bridge layer in `apps/mobile/rust/src/api/mod.rs` (~840 lines) is still pull-based — Dart calls methods like `get_lineup()`, `get_group_state()`, `get_chat_messages()` to fetch data on demand.

Now we replace these with reactive streams using flutter_rust_bridge v2's `Stream<T>` support.

## Task

### Streams in the bridge (`apps/mobile/rust/src/api/mod.rs`)

1. **`watch_lineup(festival_id: String) -> impl Stream<Item = LineupDto>`**
   - Emit the current lineup state immediately (from local Yrs doc)
   - Re-emit whenever the doc is updated (from gossip or sync)
   - Use a `tokio::sync::watch` channel internally: DocManager notifies on update

2. **`watch_group_state(group_id: String) -> impl Stream<Item = GroupStateDto>`**
   - Same pattern: emit current, re-emit on change

3. **`watch_chat(topic: String, last_n: u32) -> impl Stream<Item = Vec<ChatMessageDto>>`**
   - Emit the last N messages immediately
   - Emit new messages as they arrive (appended to the list, or just the new ones — decide based on what's ergonomic for Dart)

4. **`watch_sync_status() -> impl Stream<Item = SyncStatusDto>`**
   - Emit current sync state of all resources
   - Re-emit when any resource's sync state changes

5. **Write commands** — these should remain as async functions (not streams):
   - `check_in`, `toggle_star`, `send_chat`, `send_group_chat`, etc.
   - Each write: mutate local state → notify the relevant watch channel → queue gossip broadcast
   - The stream emits the updated state BEFORE gossip delivery

### Internal notification mechanism

6. Add a notification layer to DocManager (or OffbeatNode):
   - When a Yrs doc is updated (locally or from gossip), notify watchers
   - When a chat message is inserted, notify watchers
   - Use `tokio::sync::watch::Sender<T>` per resource instance

7. FRB `Stream<T>` support: flutter_rust_bridge v2 supports returning `impl Stream` from Rust functions. The stream is bridged to a Dart `Stream` automatically. Ensure the function signatures are compatible with FRB codegen.

### Cleanup

8. Remove pull-based methods that are replaced by streams:
   - `get_lineup` → replaced by `watch_lineup`
   - `get_group_state` → replaced by `watch_group_state`
   - `get_chat_messages` / `get_chat_history` → replaced by `watch_chat`
   - Keep `get_stars` and `toggle_star` (these are local-only, not synced)

9. Simplify DTOs where possible — if core types already have the right shape, re-export them instead of converting.

10. Remove the manual gossip broadcast code from each write method. Instead, writes should go through the SyncOrchestrator which handles broadcast automatically.

### Tests

11. Test that a local write triggers a stream emission.
12. Test that applying a remote update triggers a stream emission.

## Important

- Read `apps/mobile/rust/src/api/mod.rs` thoroughly — understand what each method does before removing it
- Read flutter_rust_bridge v2 docs for Stream support patterns
- Run `flutter_rust_bridge_codegen generate` after changes to regenerate Dart bindings
- The Dart side will be rewired in Phase 6 — for now just ensure the Rust API compiles and FRB codegen succeeds

## Branch

`git spice branch create phase-5-frb-streams`

## Commit

`git spice commit create -m "replace pull-based bridge API with reactive FRB streams"`

## Validate

`pnpm turbo check --ui stream --output-logs errors-only`

## Report

Files changed, methods removed/added, FRB codegen success, pass/fail.
```

---

## Phase 6: Dart UI Rewire to Streams

**Goal:** Replace all pull-based data fetching in Flutter with stream subscriptions. Remove manual refresh patterns.

**Dependencies:** Phase 5

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 6 of the Reactive Resource Layer for the Offbeat festival app.

## Context

Phase 5 replaced the pull-based Rust bridge API with reactive streams: `watch_lineup`, `watch_group_state`, `watch_chat`, `watch_sync_status`. Now the Flutter UI needs to be rewired to consume these streams instead of manually fetching data.

## Task

### Festival detail screens (`apps/mobile/lib/screens/festival_detail/`)

1. `festival_detail_screen.dart` — replace any manual lineup fetching with `StreamBuilder<LineupDto>` on `node.watchLineup(festivalId)`.

2. `gantt_view.dart` — receive lineup data from the stream (passed down from festival_detail_screen or subscribed directly).

3. `stage_tabs_view.dart`, `day_tabs_view.dart` — same: driven by the lineup stream.

4. `clash_radar_view.dart` — same, plus starred sets (which may remain local-only via `getStars`).

### Festival list (`apps/mobile/lib/screens/festival_list/`)

5. `festival_list_screen.dart` — this currently fetches from the server REST endpoint. This stays as-is (festival discovery is REST, not gossip). No change needed unless it also reads local state.

### Services

6. Simplify `festival_service.dart` — it should become a thin wrapper or be removed if the streams replace its functionality entirely.

7. Simplify `festival_admin_service.dart` — admin operations stay as async calls (not streams).

### Chat screens (if they exist)

8. Wire any chat UI to `watch_chat(topic, lastN)` streams.

### General patterns

9. Replace `FutureBuilder` with `StreamBuilder` where data is now streamed.
10. For screens that need initial data + live updates, the stream handles both (it emits current state immediately then updates).
11. Remove any manual "refresh" buttons or pull-to-refresh for synced data.
12. Keep pull-to-refresh for REST-fetched data (festival list).

## Important

- Read each screen file before modifying to understand the current data flow
- The FRB-generated Dart files are in `apps/mobile/lib/src/rust/` — import from there
- Run `flutter analyze` to check for errors
- Run `cd apps/mobile && flutter build apk --debug` to verify compilation (or just `flutter analyze` if no device)
- Do NOT modify Rust code — this phase is Dart only

## Branch

`git spice branch create phase-6-dart-streams`

## Commit

`git spice commit create -m "rewire Flutter UI to use reactive FRB streams"`

## Validate

`pnpm turbo check --ui stream --output-logs errors-only`

## Report

Files changed, patterns replaced (FutureBuilder→StreamBuilder count), pass/fail.
```

---

## Phase 7: WS Relay → iroh Custom Transport

**Goal:** Replace `ws_relay.rs` with an iroh custom transport that speaks WebSocket to the DO.

**Dependencies:** Phase 6

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 7 of the Reactive Resource Layer for the Offbeat festival app.

## Context

The current `crates/core/src/ws_relay.rs` (~350 lines) is a hand-built WebSocket client that connects to the Festival Durable Object. It has its own reconnect logic, subscribe/unsubscribe protocol, and message dispatching.

iroh (version 1.0.0-rc.0) supports custom transports via the `unstable-custom-transports` feature (already enabled in `crates/core/Cargo.toml`). We want to replace the WebSocket relay with an iroh custom transport, making the DO just another peer in iroh's transport hierarchy.

## Research first

Before implementing, read:
- The iroh custom transport API: look at the `iroh` crate's docs for custom transport traits
- The Tor transport example referenced in the iroh 0.97 blog post
- The existing `crates/core/src/transport/` directory (if it exists) for any prior transport work

The key traits to implement are the datagram-level transport traits that iroh provides. WebSocket frames would carry iroh datagrams.

## Task

1. Create `crates/core/src/ws_transport.rs` implementing iroh's custom transport traits:
   - The transport connects to a WebSocket URL
   - Each WebSocket binary frame carries one iroh datagram
   - The transport handles reconnection (or lets iroh handle it — check what the API requires)

2. Wire the custom transport into the endpoint builder:
   ```rust
   Endpoint::builder(presets::N0)
       .add_custom_transport(ws_transport)
       .alpns(vec![GOSSIP_ALPN.to_vec()])
       .bind()
       .await?
   ```

3. Delete `ws_relay.rs` — all its functionality is now handled by:
   - iroh (transport, reconnect, multiplexing)
   - SyncOrchestrator (subscribe, catchup, dispatch)
   - gossip_manager (broadcast)

4. Update `lib.rs` — remove `WsRelaySink` from `OffbeatNode`, replace with the custom transport configuration.

5. Update the bridge (`apps/mobile/rust/src/api/mod.rs`) — `connect_relay` should configure the custom transport instead of calling `ws_relay::connect`.

6. The DO's WebSocket protocol may need to adapt. Since the custom transport carries iroh datagrams (not the current JSON protocol), one of two approaches:
   - **Option A (recommended for now):** The custom transport translates between iroh datagrams and the existing DO JSON protocol. This avoids changing the DO.
   - **Option B:** Change the DO to speak raw datagrams. Cleaner but requires more server changes.

   Choose Option A unless Option B is trivially simple.

## Tests

7. Unit test the transport: mock WebSocket, verify datagrams round-trip.
8. Verify existing two-node sync tests still pass.

## Important

- This is the highest-risk phase. Read the iroh custom transport API carefully before writing code.
- If the API is too unstable or doesn't support what we need, document what's missing and implement a minimal adapter instead of a full custom transport.
- Do not break existing functionality — if the custom transport can't fully replace ws_relay, keep ws_relay as a fallback and document what's left.

## Branch

`git spice branch create phase-7-ws-transport`

## Commit

`git spice commit create -m "replace ws_relay with iroh custom transport over WebSocket"`

## Validate

`pnpm turbo check --ui stream --output-logs errors-only`

## Report

Files created/deleted, approach taken (Option A or B), any API limitations discovered, pass/fail.
```

---

## Phase 8: Cleanup and Test Consolidation

**Goal:** Delete dead code, consolidate boilerplate tests, verify all metrics from the PRD.

**Dependencies:** Phase 7

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing Phase 8 (final) of the Reactive Resource Layer for the Offbeat festival app.

## Context

Phases 1-7 built the resource abstraction, sync protocols, orchestrator, streams, Dart rewire, and custom transport. Now we clean up.

## Task

### Dead code removal

1. Run `cargo clippy --workspace --all-targets -- -D warnings` and fix all warnings.
2. Look for unused imports, dead functions, and unreachable code across:
   - `crates/core/src/gossip_manager.rs` — should be < 100 lines of non-test code now
   - `crates/core/src/db/mod.rs` — should be < 300 lines now
   - `apps/mobile/rust/src/api/mod.rs` — should be < 400 lines now
3. Remove any compatibility shims, commented-out code, or TODO markers from prior phases.
4. If `ws_relay.rs` still exists as a fallback, evaluate if it can be fully deleted.

### Test consolidation

5. Identify boilerplate-heavy tests:
   - Tests in `db/mod.rs` that manually construct SQL and assert row-by-row
   - Tests in `gossip_manager.rs` that tested the now-deleted GossipMessage dispatch
   - Tests in `chat.rs` and `groups.rs` that duplicate logic now handled by the Resource abstraction

6. Replace with tests against the abstraction:
   - Test the SyncOrchestrator end-to-end: register resources, sync with mock peer, verify state
   - Test the Resource trait implementations: correct topic, priority, visibility
   - Test the stream notification: write → stream emits

7. Ensure test count is reasonable — fewer tests but same coverage. No orphaned tests.

### Metrics verification

8. Count and report:
   - `gossip_manager.rs` non-test lines (target: < 100)
   - `db/mod.rs` lines (target: < 300)
   - `apps/mobile/rust/src/api/mod.rs` lines (target: < 400)
   - Number of distinct sync mechanisms (target: 2 — SV exchange + HWM)
   - Any remaining pull-based data fetching in Dart (target: 0 synced resources)

### Final validation

9. `cargo test --workspace` — all tests pass
10. `cargo clippy --workspace --all-targets -- -D warnings` — no warnings
11. `pnpm turbo check --ui stream --output-logs errors-only` — passes

## Branch

`git spice branch create phase-8-cleanup`

## Commit

`git spice commit create -m "cleanup dead code, consolidate tests, verify metrics"`

## Validate

`pnpm turbo check --ui stream --output-logs errors-only`

## Report

Final metrics, lines removed, tests removed/added, overall pass/fail, any remaining TODOs.
```

---

## Summary

| Phase | Goal | Key Deliverable | Depends On |
|-------|------|-----------------|------------|
| 1 | Resource trait | `resource.rs` with trait + 4 impls + registry | — |
| 2 | CRDT sync | State vector exchange replaces gossip_log for CRDTs | 1 |
| 3 | Chat sync | High-water mark replaces sinceSeq for chat | 2 |
| 4 | Orchestrator | SyncOrchestrator + simplified gossip_manager | 3 |
| 5 | FRB streams | `Stream<T>` per resource in bridge layer | 4 |
| 6 | Dart rewire | Flutter screens use StreamBuilder | 5 |
| 7 | WS transport | iroh custom transport replaces ws_relay | 6 |
| 8 | Cleanup | Dead code deletion, test consolidation, metrics | 7 |
