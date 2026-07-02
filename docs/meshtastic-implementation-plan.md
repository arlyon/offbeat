<!-- markdownlint-disable MD013 -->

# Meshtastic Implementation Plan

This document is the accountability checklist for Offbeat's Meshtastic route. The goal is not to pretend Meshtastic is a full iroh transport. It is a constrained physical route for the shared Offbeat sync protocol, carried through a Bluetooth-paired Meshtastic radio.

## Current honest state

Implemented:

- Official Meshtastic protobuf dependency: `meshtastic_protobufs`.
- Offbeat compact packet format for `PRIVATE_APP` payload bytes.
- `ToRadio` encoding with official `MeshPacket`, `Data`, `PortNum::PrivateApp`, and Meshtastic priority enum.
- `FromRadio` decoding with official protobufs and extraction of `PRIVATE_APP` payloads.
- Fragmentation/reassembly and dedupe keyed by `(topic_tag, message_id)`.
- A Rust lifecycle manager that scans, connects, queues outbound frames, flushes TX, polls RX, reconnects/backoffs, and reports status counters.
- Android bridge scaffold that uses the existing BLE central for scan/connect/discover/subscribe/write against Meshtastic service and ToRadio/FromRadio characteristics.

Not implemented yet:

- Rust ↔ Android JNI binding for `MeshtasticSidecarBridge`.
- iOS CoreBluetooth bridge for Meshtastic sidecar BLE.
- Real device notification callback plumbing from `FromRadio` into `MeshtasticSidecar::poll_rx`.
- Runtime integration from domain changes (festival update, group update, group chat) into `MeshtasticSidecar::queue_frame`.
- Applying inbound compact frames back into DocManager/Chat DB.
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
Offbeat resource update
  -> TransportProfile::Constrained policy
  -> Offbeat compact PRIVATE_APP payload packet(s)
  -> Meshtastic protobuf ToRadio/MeshPacket/Data(portnum=PRIVATE_APP)
  -> BLE ToRadio characteristic
  -> Meshtastic firmware / LoRa mesh
  -> BLE FromNum notification
  -> repeated BLE FromRadio characteristic reads until empty
  -> Meshtastic protobuf FromRadio/MeshPacket/Data(portnum=PRIVATE_APP)
  -> Offbeat compact packet reassembly/dedupe
  -> Offbeat DocManager/Chat application path
```

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
- Apply inbound compact bodies through the same DocManager/ChatManager semantics used by other routes.
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
