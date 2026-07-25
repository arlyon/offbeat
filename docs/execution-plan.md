# Offbeat execution plan

> Current implementation sequence. Product behaviour lives in `prd.md`; sync semantics live in `sync-patterns.md`; active dependencies live in the Beads workspace at `../beads/offbeat`.

## Objective

Complete Offbeat's local-first sync surface without creating transport-specific domain models. A user who has previously opened a festival must be able to use its lineup, groups, check-ins, saved sets, pins, and chat from local state, then converge with peers or the server when a route becomes available.

The top-level event registry remains server-authoritative and may only show cached results offline. Peer bootstrap of never-seen events is out of scope.

## Current foundation

The repository already contains substantial infrastructure. Extend and verify it rather than recreating it:

- Flutter UI and flutter_rust_bridge bindings.
- Rust `ResourceRegistry` for festival state, group state, public chat, and group chat.
- `SyncOrchestrator` with Yrs state-vector and append-log high-water-mark catch-up.
- SQLite persistence with WAL and busy timeout.
- Yrs nested maps for members, stars, and pins.
- iroh endpoint identity persistence and direct-connection management.
- WebSocket relay with reconnect/backoff.
- Transport profiles that suppress inappropriate bulk sync.
- Official Meshtastic protobuf framing, fragmentation, lifecycle management, and an Android hardware harness.
- Wi-Fi Aware platform scaffolding with runtime capability detection.

This list is evidence of a foundation, not a declaration that each path works end to end.

## Architectural invariants

1. **One resource model.** `FestivalState`, `GroupState`, `StageChat`, and `GroupChat` retain the same IDs, privacy rules, and apply semantics on every route.
2. **Local first.** Read local SQLite before networking. Persist accepted local mutations before announcing them.
3. **Trusted origin.** Verify festival-authority signatures before applying festival state. Never treat relay delivery as authority.
4. **Private groups.** Encrypt group state and group chat with AES-256-GCM. Group keys and membership remain opaque to the Durable Object and unrelated peers.
5. **Protocol by data shape and authority.** Group CRDTs use bilateral state-vector exchange. Festival CRDTs use authority-signed deltas and full checkpoints; peers never synthesise trusted festival diffs. Append logs use per-writer high-water marks.
6. **Profile by route.** Full routes may carry snapshots and bounded history; low-bandwidth routes are capped; constrained routes carry compact, prioritised operations and no bulk history.
7. **No generated edits.** Change Rust bridge APIs, run FRB code generation, then validate generated output.

## Work sequence

### 1. Land the Meshtastic group-chat debug path

Review the current group-chat E2E and broad formatting diff. Confirm that generated bindings match the bridge API, targeted Rust/Flutter checks pass, and hardware-only limitations are recorded.

This proves compact encrypted message framing and local apply semantics. It does not complete production background queueing.

### 2. Complete signed festival state

Make lineup, stages, cancellations, metadata, and announcements a complete `FestivalState` path:

- open from local persistence without network;
- subscribe when a festival is opened;
- receive live authority-signed deltas and late-join signed full checkpoints on capable routes;
- verify the configured festival authority, document ID, update kind, and monotonic authority sequence before apply;
- relay only persisted verified envelopes through peers; reject unsigned state-vector diffs;
- broadcast and persist idempotently;
- use compact signed absolute updates only on constrained routes.

The Flutter UI must never add a REST lineup fallback.

### 3. Complete the encrypted group-state foundation

On group create/join:

- persist the group key;
- register `GroupState` and `GroupChat` immediately;
- subscribe and initiate catch-up;
- perform private shared-group discovery without revealing membership;
- converge group name/metadata and member identity fields.

Use this foundation for check-ins, per-user shared stars, and pins. Personal stars remain private local data unless explicitly shared into a group.

### 4. Complete production group chat

Move group chat from debug-only calls into the normal resource path:

- local persist-first send;
- encrypted broadcast through active peers;
- bounded HWM catch-up;
- normal UI watcher notifications;
- compact short-message representation on constrained routes;
- restart-safe retries and apply-once semantics.

### 5. Resolve append-log ordering and public trust

Before shipping public chat:

- replace authoritative wall-clock ordering with a deterministic causal order such as an HLC tuple;
- keep wall time only for display;
- distinguish festival-authority signatures from attendee authorship signatures;
- decide how cached MainDO attestations affect offline trust, expiry, and unknown-key presentation.

Then complete signed stage/general/campsite chat with topic-interest filtering, live exchange, bounded recent catch-up, and online pagination for older history.

### 6. Cache the server event registry

Persist successful `/festivals` discovery responses and render them offline. A fresh install with neither network nor cache shows an honest offline/empty state. Do not accept peer-introduced events.

### 7. Decide physical transport integration from prototypes

Keep resource semantics independent while testing:

1. Real state-vector and bounded chat sync over the existing iroh BLE transport.
2. Complexity and operational behaviour of making the JavaScript Durable Object WebSocket an iroh custom transport.
3. Byte, latency, and airtime cost of native iroh framing versus compact resource frames over Meshtastic.
4. Wi-Fi Aware/Direct coverage and platform behaviour on representative devices.

Prefer iroh integration where it is reliable and efficient. Do not force bulk QUIC traffic onto LoRa solely for conceptual uniformity.

### 8. Harden persistence and relay recovery

Close the remaining correctness risks:

- accepted local mutations and outbound queues survive app kill;
- retries are idempotent and expire deliberately;
- catch-up persistence is batched;
- blocking SQLite work does not stall async network reactors;
- WebSocket reconnect uses jitter;
- Durable Object subscriptions restore after hibernation;
- public/state ingress is authenticated;
- private opaque ingress has payload, rate, and retention limits;
- no destructive Yrs snapshot replacement is introduced as “compaction”.

### 9. Run the multi-device acceptance matrix

Use at least two devices for route-level validation. Cover:

- cached cold start;
- festival-state late join and tampered-signature rejection;
- group create/join, metadata, membership, check-ins, shared stars, and pins;
- group/public chat, bounded catch-up, and clock skew;
- duplicate delivery, app kill, route loss, fallback, and reconnect;
- profile suppression on BLE and Meshtastic;
- unsupported Wi-Fi hardware reported honestly.

## Validation gates

### Fast iteration

```bash
cargo test -p offbeat-core <targeted-test>
cargo check -p offbeat_bridge
cd apps/mobile && flutter analyze
pnpm -F @offbeat/server test
```

Run Android cross-target checks when bridge or native networking code changes. Run FRB code generation whenever the bridge API changes.

### Completion

```bash
pnpm check
```

Also run `git diff --check`, inspect generated changes, and review the final diff for secrets, accidental formatting scope, and stale documentation.

### Field performance targets

| Behaviour | Target |
|---|---:|
| Cached cold start | <2 seconds to interactive |
| Full-route festival late join | <5 seconds to interactive |
| Group join on Wi-Fi | <3 seconds |
| BLE check-in propagation | <5 seconds |
| Meshtastic check-in propagation | <30 seconds |
| Gantt scrolling | 60 fps |

## Definition of done

A work item is complete only when its behaviour is observable through the normal UI/resource path, persists across restart, converges after a partition, rejects invalid input at the trust boundary, and has targeted automated tests plus the required route-level evidence.
