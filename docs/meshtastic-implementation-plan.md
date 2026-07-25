<!-- markdownlint-disable MD013 -->

# Meshtastic Implementation Plan

This document is the accountability checklist for Offbeat's Meshtastic route. Meshtastic is always classified as a constrained physical route carried through a Bluetooth-paired radio. The production baseline uses compact Offbeat resource frames; a native iroh framing experiment remains welcome, but must demonstrate acceptable bytes, fragmentation, latency, and airtime before replacing the compact path.

## Current honest state

Implemented:

- Official Meshtastic protobuf dependency: `meshtastic_protobufs`.
- Offbeat compact packet format for `PRIVATE_APP` payload bytes.
- `ToRadio` encoding with official `MeshPacket`, `Data`, `PortNum::PrivateApp`, and Meshtastic priority enum.
- `FromRadio` decoding with official protobufs and extraction of `PRIVATE_APP` payloads.
- Fragmentation/reassembly and dedupe keyed by `(topic_tag, message_id)`.
- A Rust lifecycle manager that scans, connects, queues outbound frames, flushes TX, polls RX, reconnects/backoffs, and reports status counters.
- Android FRB hardware harness that scans/connects with `blew`, subscribes to `FromNum`, drains `FromRadio`, writes `ToRadio`, and reports decoded frames.
- Compact encrypted group-chat bodies that can be sent over Meshtastic and applied into the local chat DB for matching local groups.

Not implemented yet:

- Background Rust ↔ Android sidecar task that keeps a selected Meshtastic radio connected outside the debug harness.
- iOS CoreBluetooth bridge for Meshtastic sidecar BLE.
- Runtime integration from normal domain changes (festival update, group update, group chat) into a persistent Meshtastic queue.
- Applying inbound festival/group-state compact frames back into DocManager; group chat apply is wired in the harness.
- Persistent queue/retry across app restarts.
- Real airtime/rate limiting beyond queue limits and payload/fragment caps.
- Hardware tests against actual Meshtastic firmware.

## Official app findings

The official Meshtastic Android and Apple apps use BLE GATT, not classic Bluetooth SPP, for mobile radio access:

- Service UUID: `6ba1b218-15a8-461f-9fa8-5dcae273eafd`
- `ToRadio` write characteristic: `f75c76d2-129e-4dad-a1dd-7866124401e7`
- `FromNum` notify characteristic: `ed9da18c-a800-4f66-a670-aa7547e34453`
- `FromRadio` read characteristic: `2c55e69e-4993-11ed-b878-0242ac120002`
- `LogRadio` notify characteristic: `5a3d6e49-06e6-4423-9944-e9de8cdf9547`

The important operational pattern is: subscribe to `FromNum`, then drain by repeatedly reading `FromRadio` until an empty read. `ToRadio` and `FromRadio` values are serialized Meshtastic protobufs.

## Rust crate evaluation

The `meshtastic = 0.1.9` Rust crate does implement useful serial/TCP/BLE helpers. Its BLE helper uses `btleplug`, scans the official service UUID, writes protobuf `ToRadio` bytes to `ToRadio`, listens to `FromNum`, and drains `FromRadio`. However, it is GPL-3.0 and primarily targets host Rust environments. Offbeat should not link it into the mobile core unless we explicitly accept GPL obligations.

Decision for now:

- Use `meshtastic_protobufs` directly for protocol types.
- Keep Android/iOS BLE connection code in platform bridges using official app behavior.
- Treat `meshtastic = 0.1.9` as a reference implementation only, unless licensing is revisited.

## Protocol ownership

Do not hand-roll Meshtastic's Bluetooth/radio envelope. Offbeat uses:

- `meshtastic_protobufs::meshtastic::ToRadio`
- `meshtastic_protobufs::meshtastic::FromRadio`
- `meshtastic_protobufs::meshtastic::MeshPacket`
- `meshtastic_protobufs::meshtastic::Data`
- `meshtastic_protobufs::meshtastic::PortNum::PrivateApp`

Offbeat owns only the bytes inside `Data.payload` for `PRIVATE_APP`.

## Layering

```text
Offbeat logical resource
  -> shared IDs, trust, dedupe, and apply semantics
  -> TransportProfile::Constrained policy
  -> compact PRIVATE_APP payload packet(s)
  -> Meshtastic protobuf ToRadio/MeshPacket/Data(portnum=PRIVATE_APP)
  -> BLE ToRadio characteristic
  -> Meshtastic firmware / LoRa mesh
  -> BLE FromNum notification
  -> repeated BLE FromRadio characteristic reads until empty
  -> Meshtastic protobuf FromRadio/MeshPacket/Data(portnum=PRIVATE_APP)
  -> compact packet reassembly/dedupe
  -> normal Offbeat DocManager/Chat application path
```

This baseline keeps the data structures transport-agnostic: only representation and scheduling differ. An all-iroh prototype must preserve the same profile suppression and be compared against this baseline on real radios.

## Implementation phases

### Phase 1 — Protocol hardening

- Keep using `meshtastic_protobufs` for all `ToRadio`/`FromRadio` handling.
- Add golden protobuf fixtures captured from a real Meshtastic device.
- Validate priority mapping:
  - festival update -> `Priority::Alert`
  - group update -> `Priority::High`
  - group chat -> `Priority::Reliable`
  - festival chat -> `Priority::Background`
- Add negative tests for non-`PRIVATE_APP`, encrypted payload variants, malformed protobufs, and oversized fragments.

### Phase 2 — Android sidecar bridge

- Add JNI functions from Rust to `MeshtasticSidecarBridge`.
- Start scan filtered to `6ba1b218-15a8-461f-9fa8-5dcae273eafd`.
- Connect to preferred bonded radio or first selected radio.
- Discover services.
- Subscribe to `FromNum` characteristic.
- On each `FromNum` notification, repeatedly read `FromRadio` until empty.
- Write `ToRadio` protobufs to `ToRadio` characteristic.
- Forward read `FromRadio` protobuf bytes into a Rust channel consumed by `MeshtasticSidecar::poll_rx`.
- Expose status to Flutter without editing generated FRB files by hand.

### Phase 3 — iOS sidecar bridge

- Implement CoreBluetooth scan/connect for the Meshtastic service UUID.
- Discover `ToRadio`/`FromNum`/`FromRadio` characteristics.
- Subscribe to `FromNum`, drain `FromRadio`, and write `ToRadio` protobufs.
- Mirror Android status/errors.

### Phase 4 — Offbeat integration

- Map outbound resources to compact bodies:
  - P0 festival signed update/alert
  - P1 encrypted group update/check-in/pin
  - P2 encrypted short group chat
  - P3 festival chat only when idle
- Apply inbound compact bodies through the same DocManager/ChatManager semantics used by other routes. Group chat is implemented in the hardware harness; group state and festival signed updates remain.
- Ensure dedupe across WebSocket/BLE/Wi-Fi Aware/Meshtastic.

### Phase 5 — Field hardening

- Persistent queue with expiry.
- Token-bucket airtime limiter.
- Firmware compatibility matrix.
- Real battery and airtime measurements.
- Manual two-radio tests at festival-like distances.

## Test plan

### Unit tests

- Offbeat PRIVATE_APP packet encode/decode round trip.
- Fragmentation and out-of-order reassembly.
- Duplicate fragment/message drops.
- Suppression of bulk CRDT sync and chat history.
- `ToRadio` protobuf encode wraps `MeshPacket.payload_variant=Decoded(Data)` with `PortNum::PrivateApp`.
- `FromRadio` protobuf decode ignores non-`PRIVATE_APP` packets and extracts Offbeat payloads from valid packets.
- Priority mapping to Meshtastic `MeshPacket.Priority`.

### Integration tests with fake adapter

- Start -> scan -> connect -> connected state.
- Queue frame -> tick -> writes `ToRadio` protobuf to adapter.
- Push `FromRadio` protobuf -> tick -> returns reassembled `OffbeatSyncFrame`.
- Disconnect -> tick -> backoff/reconnect behavior.
- Queue full fails closed.

### Hardware tests

Use the in-app **You → Meshtastic Test Rig** harness before claiming full app E2E.

Single-radio smoke test:

1. Pair/configure the phone with a Meshtastic radio.
2. Open **Meshtastic Test Rig**.
3. Tap **Scan radios** and select the radio.
4. Tap **Send + listen**.
5. Pass if the report shows `connected`, discovered services, `sent_fragments > 0`, and no BLE/protobuf error.

Two-radio Offbeat `PRIVATE_APP` test:

1. Pair/configure phone A with radio A and phone B with radio B.
2. Open **Meshtastic Test Rig** on phone B, scan/select radio B, tap **Listen 30s**.
3. Within that window, use phone A to scan/select radio A and tap **Send + listen**.
4. Pass if phone B reports `private_app > 0` and at least one decoded frame whose body matches the debug payload.

Two-radio encrypted group-chat test:

1. Both phones must have joined the same group for the current festival.
2. Phone B: open **Meshtastic Test Rig**, scan/select radio B, tap **Listen + apply**.
3. Phone A: open **Meshtastic Test Rig**, enter the group ID and short message, tap **Send group chat**.
4. Pass if phone B reports `applied_group_chats > 0` and the message appears in the normal Social group chat history.

Full app E2E, after background queue integration:

1. Pair phone with Meshtastic radio.
2. Verify service and `ToRadio`/`FromNum`/`FromRadio` characteristics are discovered.
3. Verify `FromNum` notifications trigger `FromRadio` drain reads until empty.
4. Send a P0 festival alert; verify another radio/phone receives it.
5. Send P1 group check-in; verify decrypt/apply and dedupe.
6. Send duplicate packet; verify one application only.
7. Power-cycle radio; verify reconnect and queue behavior.
8. Disable WebSocket/Wi-Fi/BLE peer routes; verify Meshtastic still carries constrained updates.

## Acceptance criteria

Meshtastic is not considered done until a real Android phone connected to a real Meshtastic radio can send and receive Offbeat `PRIVATE_APP` protobuf payloads, and those payloads are applied through Offbeat's normal resource sync paths.
