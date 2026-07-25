# Direct P2P connectivity and multi-path bootstrap

> Current requirements and design history. The original phase-by-phase agent prompts were removed after implementation began. Physical transport selection remains gated by the prototypes in `execution-plan.md`.

## Problem

A WebSocket relay is useful for online catch-up but cannot be the only way two nearby phones find and synchronise with each other. Offbeat needs stable peer identity, independent discovery hints, direct connection attempts, route promotion, and graceful relay fallback.

Direct connectivity must not create route-specific festival, group, or chat schemas.

## Scope

- Persist a stable iroh endpoint identity across restarts.
- Discover candidate peers through multiple independent hints.
- Connect directly where an eligible route exists.
- Promote constrained encounters to higher-bandwidth routes.
- Register and synchronise only resources the local user may access.
- Report peer and route status to Flutter.
- Continue through the Durable Object relay when direct connectivity is unavailable.

## Non-goals

- Peer discovery of a never-seen festival/event. The top-level event registry remains server-authoritative and locally cached.
- Revealing group IDs, group keys, or membership during ambient discovery.
- Guaranteeing that every device supports Wi-Fi Aware or Wi-Fi Direct.
- Requiring identical wire framing on every physical link.
- Replacing the current WebSocket relay before an alternative proves equivalent behaviour.

## Identity

The iroh secret key is generated once and stored in the local database or platform-protected storage. Its public endpoint ID is stable across application restarts.

A discovery hint may expose a bounded prefix of the endpoint ID, but a connection attempt must resolve and verify the complete identity before treating the peer as connected. Public keys are not secrets; possession of the corresponding private key proves endpoint identity.

## Bootstrap hints

No single hint is required on every platform.

### Server-known peers

For a festival the client already knows, signed festival state or relay metadata may include recent endpoint IDs and route hints. These are candidates, not trusted content authorities.

### BLE identity discovery

Nearby devices advertise the Offbeat/iroh BLE service. A scanner observes a key prefix and may read the full endpoint ID from the GATT identity characteristic before asking iroh to connect.

BLE identity exchange must work without the Durable Object and without a previously synchronised peer list. It does not grant access to private group resources.

### Existing iroh discovery

When internet or LAN discovery is available, iroh may resolve known endpoint IDs through its configured discovery/relay mechanisms. This complements local discovery rather than replacing it.

### Durable Object relay

The Festival DO remains a pragmatic WebSocket peer adapter and persisted catch-up source. It is always considered untrusted delivery infrastructure; clients verify festival authority, attendee signatures, and group encryption independently.

## Connection manager

The connection manager owns candidate aggregation and connection lifecycle:

1. Load the stable endpoint identity.
2. Collect candidate endpoint IDs and route hints from active discovery sources.
3. Deduplicate candidates by complete endpoint ID.
4. Avoid connecting to self.
5. Attempt eligible direct routes with bounded concurrency and backoff.
6. Track discovery, connection, route profile, gossip/resource readiness, and staleness separately.
7. Notify Flutter when the externally visible status changes.
8. Retain relay fallback and retry direct routes when conditions improve.

A BLE scan sighting is not a successful iroh connection. A direct iroh connection is not proof that resource registration and catch-up succeeded. Status must preserve these distinctions.

## Route promotion

A constrained encounter may exchange enough public capability information to offer a better path:

- protocol version;
- complete verified endpoint identity;
- supported routes and profiles;
- public festival interests;
- privacy-preserving shared-group discovery material.

When both peers can establish LAN, Wi-Fi Aware, Wi-Fi Direct, or another `Full` path, perform snapshots and bounded history there. Keep BLE or Meshtastic available for urgent fallback.

## Resource bootstrap

After a peer connection is ready:

- register/open festival state only for festivals already known locally;
- exchange state vectors for common CRDT resources;
- perform the private shared-group handshake before registering group resources with that peer;
- exchange chat high-water marks only for common subscribed topics;
- enforce the selected route profile before sending data.

Resource catch-up is bilateral because each peer may hold unique offline changes.

## Private group discovery

Ambient discovery must not announce raw group IDs. Peers use a fresh-session, group-key-derived challenge protocol to determine shared possession without transmitting the group key. A successful match enables normal encrypted `GroupState` and `GroupChat` registration.

See `prd-group-pubsub.md` for the group lifecycle.

## Platform routes

### BLE

The vendored `iroh-ble-transport` is the preferred starting point. Hardware tests must prove endpoint exchange, connection stability, state-vector sync, bounded chat catch-up, restart behaviour, and contention with the Meshtastic sidecar.

### Wi-Fi Aware

Use where the OS and firmware expose it. Capability checks are mandatory. A platform-provided IP/UDP path may be handed to iroh; an object-based connection may require a custom adapter.

### Wi-Fi Direct

Consider as a coverage fallback when Wi-Fi Aware availability is insufficient. Android and iOS expose materially different APIs, so implementation cost and user-consent behaviour require a prototype before commitment.

### Meshtastic

Meshtastic can advertise or carry urgent resource operations but is not suitable for endpoint-list floods, snapshots, or history. Native iroh framing remains an experiment, not an assumption.

## Security

- Stable endpoint identity authenticates the peer connection, not application content.
- Festival state still requires a festival-authority signature.
- Public chat still requires attendee authorship verification.
- Group resources still require possession of the AES group key.
- Discovery hints are untrusted and rate-limited.
- Private shared-group probes use fresh nonces and reveal no reusable membership identifier.
- Candidate tables and logs must be bounded to resist nearby-radio flooding.

## Current implementation evidence

The repository contains:

- persisted endpoint identity;
- BLE discovery/transport integration and mock tests;
- a connection manager with peer status;
- iroh gossip/direct connection plumbing;
- a ResourceRegistry and SyncOrchestrator;
- WebSocket relay fallback;
- Wi-Fi Aware platform scaffolding;
- Meshtastic compact-frame and sidecar foundations.

These components still require complete normal-path and hardware validation.

## Acceptance criteria

1. Endpoint ID remains stable across restart.
2. Two previously configured devices discover each other without the Durable Object using an available local route.
3. A peer missing previously known festival state catches up from another peer; neither peer introduces a new top-level event.
4. Shared-group discovery leaks no group key or raw group ID to unrelated peers.
5. Bilateral group state and bounded chat catch-up converge idempotently.
6. Relay and direct paths may coexist without duplicate logical effects.
7. Route loss falls back without losing accepted local writes.
8. Flutter distinguishes discovery, connection, active sync, degraded profile, and offline states.
9. Unsupported Wi-Fi capability is reported honestly.
10. Multi-device timing is recorded against the targets in `execution-plan.md`.

## Open decisions

- Which physical routes pass the all-iroh prototypes?
- Is Wi-Fi Direct worth its cross-platform complexity after Wi-Fi Aware coverage is measured?
- Which peer hints belong in signed festival state versus ephemeral handshakes?
- How aggressively should direct-route attempts run under battery pressure?
