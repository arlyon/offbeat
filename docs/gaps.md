# Verified implementation gaps

> Current gaps only. Historical implementation prompts and already-fixed findings have been removed. Active dependencies are tracked in Beads under the `offbeat-t6a` epic.

## Verified foundation

The following capabilities exist in source and should be tested or extended, not reimplemented:

- Resource registry with festival/group CRDT and chat append-log resource types.
- Yrs state-vector exchange.
- Per-writer chat high-water marks and idempotent SQLite insertion.
- Nested Yrs maps for group members, shared stars, and pins.
- Normal group create/join/leave now registers both resources, publishes encrypted membership deltas, updates watchers, deregisters on leave, and rehydrates persisted resources after restart.
- Direct iroh group-state catch-up encrypts both state vectors and diffs; the WebSocket DO remains an opaque encrypted replay adapter.
- Personal saved sets persist locally, rapid toggles serialize and re-read
  durable state, and ALL/MINE/OURS schedule scopes render cached personal and
  group picks. Group membership continuously mirrors same-festival likes into
  per-user/per-set encrypted CRDT entries; create/join/restart reconcile missed
  changes. Co-liker overlays deduplicate identities across groups, member
  schedules resolve cached lineup metadata, and one overlap model drives clash
  badges, filtering, and the display-only Clash Radar.
- Stage/custom check-ins use one atomic CRDT value, converge under duplicate/reordered/concurrent delivery, persist across restart, and additively subscribe the checked-in stage chat.
- SQLite WAL, normal synchronous mode, and busy timeout.
- Persisted iroh endpoint identity.
- WebSocket reconnect/backoff.
- Transport profiles and constrained-route catch-up suppression.
- Meshtastic protobuf framing, fragmentation/reassembly, lifecycle manager, Android debug harness, and encrypted group-chat apply.
- Wi-Fi Aware capability scaffolding on Android and iOS.

An implementation existing in isolation is not proof that the normal Flutter → Rust → persistence → peer → watcher path works end to end.

## P0: signed festival state needs product-path completion

**Tracking:** `offbeat-t6a.7`

The core trust and catch-up path now exists: the Festival DO applies authenticated mutations, emits signed deltas, persists signed full checkpoints, and serves those checkpoints for late joins. Rust verifies the protocol domain, document ID, update kind, monotonic authority sequence, and payload before apply, persists verified envelopes, rejects unsigned `svDiff`, and can relay a verified checkpoint over direct iroh peer catch-up.

The remaining product work is to make lineup, stage, cancellation, metadata, and announcement changes load locally and converge through the normal Flutter → Rust → watcher path. The mobile UI must not fetch lineup data through REST.

Required evidence:

- cached festival opens with no network;
- a live signed delta and late-join signed checkpoint reach identical state;
- direct peers relay only previously verified authority envelopes;
- forged, unknown-authority, duplicate, and reordered updates are handled safely;
- constrained routes carry only bounded signed operations;
- two real clients display the same state before and after restart.

## P1: shared pins need normal-path convergence

**Tracking:** `offbeat-t6a.8`

The pin CRDT mutation exists, but it still needs proof through the regular UI and sync orchestrator.

## P1: production group chat is not wired

**Tracking:** `offbeat-t6a.9`; the `offbeat-c8t` debug precursor is complete.

Meshtastic group-chat send/apply currently proves a debug path. Production behaviour still needs:

- normal local persist-first send;
- resource registration and subscriptions;
- encrypted broadcast on active routes;
- bounded HWM catch-up;
- standard chat watcher notifications;
- restart-safe outbound retries;
- deduplication across WebSocket, iroh, BLE, Wi-Fi, and Meshtastic.

## P1: append-log ordering is clock-safe

**Tracking:** `offbeat-t6a.11`

Chat now uses `(lamport_time, writer_key, writer_sequence, message_id)` for
stable authoritative order; wall-clock timestamps are display-only. Local
sequence/time allocation and persistence are atomic, incoming messages advance
the persisted per-topic clock, legacy rows migrate from writer sequence, and
history pagination uses the same indexed tuple. Catch-up filters in bounded SQL
and serves oldest missing entries first. HWM entries retain an ID-plus-Lamport
head commitment so equal-sequence conflicts are discoverable without starving
later pages.
Legacy protobuf and Meshtastic v1 messages derive logical time from writer
sequence; ID-plus-Lamport head commitments allow a later authoritative duplicate
to repair that fallback. Legacy zero/terminal counters migrate to bounded stable
synthetic sequences.

Remaining production group-chat delivery work is tracked by `offbeat-t6a.9`.

## P1: public-message trust baseline is implemented; advanced trust lifecycle remains

**Decision:** `offbeat-t6a.10`; baseline delivery: `offbeat-t6a.4`

Public attendee chat now uses domain-separated Ed25519 authorship signatures plus
cached MainDO registration attestations. Current proofs and the 7-day offline
grace produce verified trust. Missing or out-of-grace proof produces a visibly
unverified, quota-bounded message and is excluded from history catch-up. Invalid
signatures and writer-sequence equivocation are rejected or quarantined.
Festival authority and group-key possession remain separate trust domains.

The FestivalDO pins MainDO's issuer, authenticates each socket through a
festival-bound single-use challenge, verifies every public message, and sends
proofs before bounded catch-up pages. Clients persist proofs, trust, conflicts,
and a durable public-chat outbox. Advanced MainDO key rotation, revocation
snapshots, and complete cross-peer equivocation-proof propagation remain the
security-matrix follow-up defined in `auth-protocol.md`.

## Completed baseline: bounded signed public chat

**Tracking:** `offbeat-t6a.4`

Stage and campsite channels now have topic-interest wiring, signed live exchange,
bounded recent peer/relay catch-up, on-demand local pagination, trust badges,
and reconnect outbox recovery. Constrained routes never exchange bulk chat
history. Festival chat remains lower priority than festival state, group state,
and group chat.

## P1: accepted writes and queues must survive app kill

**Tracking:** `offbeat-t6a.12`

Encrypted group-state mutations now persist festival-scoped outbound intents
before publication and retry relay delivery across reconnect and restart. Leave
atomically compacts older outbound deltas into one encrypted member/star
removal while deleting the local key, chat, and cached plaintext document.
Inactive festival relay loops are explicitly stopped before replacement.

Relay rows are removed only after the server echoes the durably sequenced
message; exact retries reuse the original server sequence. Pending leave rows
retain only the encrypted wire envelope, not the departed group's key. The
remaining durability work covers chat/public writes, expiry policies, and stress
evidence. Catch-up writes should
be batched. Blocking rusqlite work must not stall Tokio network reactors under
load.

## P1: relay resilience and abuse controls need verification

**Tracking:** `offbeat-t6a.12`

Verify or complete:

- Durable Object subscription restoration after hibernation;
- jittered reconnect and bounded backoff;
- festival-state signature validation before relay/storage;
- public-chat signature validation at ingress where possible;
- payload caps, topic caps, and rate limits for opaque private blobs;
- explicit retention and mailbox quotas;
- batched catch-up persistence.

Clients must still validate all trusted-origin content independently.

## P1: physical transport boundary needs evidence

**Tracking:** `offbeat-t6a.6`

Transport-agnostic resources are settled. The unresolved question is which routes should carry native iroh framing:

- BLE has an existing iroh custom-transport implementation and should be tested on hardware.
- A JavaScript Durable Object cannot simply become a UDP/QUIC iroh node; WebSocket adaptation requires a prototype.
- Meshtastic's low throughput and 228-byte application payload make native iroh overhead questionable; compare it against compact resource frames.
- Wi-Fi Aware and Wi-Fi Direct are optional high-bandwidth routes whose value depends on real device coverage.

Do not create separate domain semantics for any route. Do not assume that identical resource semantics require identical wire packets.

## P1: Meshtastic production sidecar is incomplete

**Tracking:** `offbeat-t6a.14`, intentionally blocked on the core resource, trust, ordering, and persistence tasks.

Remaining work includes:

- persistent Android runtime sidecar outside the debug harness;
- iOS CoreBluetooth bridge;
- normal domain-change queueing;
- inbound festival/group-state apply;
- restart-safe retry and expiry;
- token-bucket airtime limiting;
- two-phone/two-radio field tests.

The GPL-3.0 `meshtastic` crate remains reference-only. Offbeat owns only `PRIVATE_APP` payload bytes and uses official protobuf envelopes.

## Resolved: event registry cache

**Tracking:** `offbeat-t6a.5`

Successful MainDO discovery responses are atomically normalized into app-side
SQLite and loaded before an online refresh. Cached data carries its fetch time,
remains visible when refresh fails, and is explicitly marked stale. A successful
refresh replaces the complete cached registry, including removals; corrupt or
missing cache data produces an honest offline/empty state.

Registered users may now publish a validated public Clashfinder source through
the online MainDO import workflow. This does not weaken registry authority:
peers still cannot introduce unseen events, and fresh-install bootstrap from
nearby devices remains out of scope.

## P1: no complete multi-device acceptance matrix

**Tracking:** `offbeat-t6a.13`

Unit and mock tests cannot prove platform route behaviour. Final validation needs at least two devices and, for Meshtastic, two configured radios. Record unsupported routes instead of substituting capability checks for peer tests.

## Superseded assumptions

The following statements from older plans are no longer authoritative:

- Tauri/Svelte is the mobile architecture.
- The Flutter client may fetch lineup data through REST.
- Wi-Fi Direct is mandatory before any offline group feature can ship.
- Every physical route must carry full QUIC packets.
- Meshtastic should carry Yrs snapshots or chat history.
- Replacing a live Yrs document with an encoded snapshot is safe compaction.
- A feature is complete because its core type or helper function exists.
