# Reactive resource layer — completion ledger

> Historical implementation phases condensed into a current-state ledger. The active implementation sequence is `execution-plan.md` and the product contract is `reactive-resource-layer-prd.md` plus `prd.md`.

## Original objective

Replace duplicated per-entity sync code with:

- a registry of typed logical resources;
- Yrs state-vector catch-up for CRDT documents;
- per-writer high-water-mark catch-up for append logs;
- one sync orchestrator over peer capabilities;
- reactive Flutter streams;
- a transport boundary that does not leak into domain schemas.

## Phase status

### Phase 1 — resource model and registry: implemented

Evidence is centred in `crates/core/src/resource.rs`:

- `FestivalState`, `GroupState`, `StageChat`, and `GroupChat` resource variants;
- `ResourceKind`, `Visibility`, and priority metadata;
- deterministic topic/resource identity;
- registry operations and resource lookup.

Remaining work is end-to-end registration timing for newly created/joined groups and complete festival-open wiring.

### Phase 2 — CRDT state-vector protocol: implemented foundation

Evidence is centred in `crates/core/src/sync_protocol.rs`, `sync.rs`, and `doc_manager.rs`.

The protocol computes state-vector differences and applies Yrs updates idempotently. Remaining work is complete trust-envelope handling, bilateral route tests, encrypted group exchange, and constrained-operation integration.

### Phase 3 — append-log high-water marks: implemented foundation

Evidence is centred in `crates/core/src/chat.rs`, `sync_protocol.rs`, `sync.rs`, and DB migrations.

Per-writer sequence catch-up and idempotent storage exist. Remaining work includes deterministic clock-skew-safe ordering, complete public trust, production group-chat wiring, and profile-specific history limits.

### Phase 4 — sync orchestrator: implemented foundation

`SyncOrchestrator` selects protocol by resource kind and reads the peer transport profile. It suppresses append-log catch-up on constrained routes.

Remaining work includes normal lifecycle integration, route promotion, restart-safe queues, and multi-device validation.

### Phase 5 — reactive FRB streams: substantially implemented

The bridge exposes resource watchers and generated Dart bindings. Generated files must continue to be produced with flutter_rust_bridge code generation rather than manual edits.

Remaining work is feature-specific watcher verification through the normal Flutter screens.

### Phase 6 — Flutter rewire: partial

Flutter uses Rust-backed state for major screens, but every offline surface still needs acceptance tests proving local-first loading, watcher updates, and honest empty/error states.

The top-level festival registry remains a REST discovery surface with a local cache. Festival lineup data must not use REST.

### Phase 7 — WebSocket/iroh transport unification: decision pending

The original plan required deleting `ws_relay.rs` and tunnelling iroh datagrams through the Durable Object WebSocket. That is no longer an accepted instruction.

The current requirement is narrower and durable: WebSocket, iroh direct paths, BLE, Wi-Fi, and Meshtastic must expose the same logical resource semantics and deduplication behaviour. Whether the WebSocket becomes a native iroh custom transport is gated by a prototype because:

- Cloudflare Workers cannot host a normal UDP/QUIC iroh endpoint;
- the custom-transport API has stability and maintenance cost;
- translating datagrams to the existing JavaScript protocol may retain nearly all current complexity;
- the existing WebSocket peer adapter already participates in SV/HWM semantics.

See `execution-plan.md` and `sync-patterns.md` for the prototype decision.

### Phase 8 — cleanup and test consolidation: active

The current cleanup is behavioural, not a line-count target:

- remove obsolete architecture instructions;
- avoid duplicate per-resource protocol implementations;
- preserve existing reliable adapters until replacements are proven;
- add stable-seam convergence and restart tests;
- validate real routes on multiple devices.

## Current acceptance criteria

The resource layer is complete when:

1. Each resource registers at the correct lifecycle event.
2. Local reads and writes work without a network route.
3. Local writes persist before broadcast and survive app termination.
4. CRDT peers converge after bilateral offline changes.
5. Append logs fill bounded gaps idempotently.
6. Festival, attendee, and group trust boundaries are distinct and enforced.
7. Transport profiles suppress inappropriate bulk data.
8. Duplicate delivery across physical routes produces one logical effect.
9. Flutter receives normal typed watcher updates without feature-specific polling loops.
10. Two-device tests confirm the available route/profile combinations.

## Validation

Targeted checks depend on the affected seam:

```bash
cargo test -p offbeat-core
cargo check -p offbeat_bridge
cd apps/mobile && flutter analyze
pnpm -F @offbeat/server test
```

The repository completion command remains:

```bash
pnpm check
```

Run FRB code generation whenever the bridge API changes, then review generated diffs.

## Non-goals

- Minimising file length at the cost of clear boundaries.
- Deleting the proven WebSocket path before a replacement passes the same tests.
- Making Meshtastic carry snapshots or chat history.
- Treating the server event registry as a peer-synchronised resource.
- Replacing Yrs with hand-written merge logic.
