# Group pubsub, private discovery, and invites

> Active gap specification. Group types, encryption, mutations, persistence, and UI components exist; this document defines the remaining normal-path lifecycle and acceptance criteria.

## Problem

A local group mutation is useful only if it is registered, persisted, broadcast, caught up, decrypted, applied, and emitted through the normal UI watcher path. Group create/join must also make state and chat subscriptions active immediately, including when peers meet without the server.

## Product scope

A festival group synchronises:

- group name and metadata;
- members and display identity;
- stage/custom-location check-ins;
- each member's explicitly shared starred sets;
- shared location pins;
- encrypted group chat.

Personal stars remain local/private unless explicitly represented in group state.

## Resource model

Each group owns two logical resources:

| Resource | Shape | Privacy | Catch-up |
|---|---|---|---|
| `GroupState` | Yrs CRDT | AES-256-GCM with group key | Encrypted bilateral state-vector exchange |
| `GroupChat` | Append log | AES-256-GCM with group key | Bounded per-writer high-water marks |

The group key is the current membership credential. The Durable Object and unrelated peers must not see plaintext group metadata, membership, state, or chat.

## Group creation

Creating a group must be one durable operation from the user's perspective:

1. Generate a random 256-bit group key.
2. Derive the stable group ID from the key.
3. Persist the group key and local group record.
4. Create/register `GroupState` and `GroupChat` resources.
5. Add the creator as a nested CRDT member entry.
6. Persist the local state before networking.
7. Subscribe through the sync orchestrator.
8. Broadcast the encrypted state update on eligible routes.
9. Emit the updated group list/state to Flutter.

A networking failure does not roll back local group creation. The outbound intent remains retryable.

## Group join

Joining from an invite must:

1. Parse and validate the versioned invite payload.
2. Confirm the festival/group context with the user.
3. Persist the group key.
4. Register both resources immediately.
5. Add the joining user as a nested CRDT member entry.
6. Start private peer discovery and relay/direct subscriptions.
7. Perform bilateral state-vector and bounded chat catch-up.
8. Emit normal group watchers.

Duplicate invite scans and retries are idempotent.

## Invite format

Invites are versioned Offbeat deep links containing the minimum material needed to join: group identity/key material and optional creator endpoint hint. The exact encoding must be parseable from QR and universal/deep links and must reject malformed lengths or unsupported versions before persistence.

The QR is a transfer of the group credential. The UI must treat it as sensitive and explain that anyone with the code can join/decrypt current group traffic.

## Mutation flow

All group-state mutations follow one path:

1. Load the group key.
2. Mutate nested Yrs structures locally.
3. Persist the resulting state/update.
4. Notify local watchers.
5. Encrypt the outgoing update.
6. Enqueue/broadcast through the registered `GroupState` resource.
7. Retry idempotently until expiry or reconciliation.

This applies to metadata, members, check-ins, shared stars, and pins. Feature-specific bridge methods must not bypass resource registration or invent separate topic logic.

## CRDT shape

Use nested shared types so independently edited fields merge:

- `members/{user_id}/{field}` for identity, status, location, and freshness;
- `stars/{user_id}/{set_id}` as per-set membership, not an atomic list;
- `pins/{pin_id}/{field}` for label, location, author, and creation metadata.

Leaving removes the member entry using a real Yrs removal before deleting the local key. Group-key rotation after removal is a future feature; the UI must not promise cryptographic revocation under the current possession model.

## Check-ins

A check-in stores either a stage ID or a custom location plus deterministic freshness/order metadata. Applying a stage check-in auto-subscribes to that public stage chat while preserving existing manual interests.

On constrained routes, check-ins use compact absolute operations and P1 priority rather than arbitrary Yrs snapshots.

## Personal and shared stars

Personal festival stars drive the user's private schedule and persist locally. Group-shared stars are explicit per-user entries in encrypted group state.

Concurrent additions/removals must merge per set. Merely starring a set personally must not leak it to a group.

## Pins

Pins are encrypted nested CRDT entries. Concurrent pins merge by pin ID. Add/remove operations flow through the same state resource and normal watchers.

## Group chat

Group chat uses the encrypted append-log resource:

- persist local messages before broadcast;
- allocate a stable message ID and per-writer sequence;
- use deterministic causal ordering independent of wall-clock skew;
- catch up a route-bounded window by HWM;
- insert duplicates idempotently;
- notify normal chat watchers;
- use compact short messages on constrained routes;
- never send history over Meshtastic.

The current Meshtastic debug harness is a protocol/hardware precursor, not the production send path.

## Private shared-group discovery

Two connected peers must determine common groups without sending raw group IDs or keys.

A session uses fresh nonces and group-key-derived tokens:

1. Each peer contributes fresh challenge material.
2. Each local group key derives a session-specific proof/token.
3. Peers compare proofs to find common possession.
4. A successful match registers only the corresponding encrypted resources with that peer.
5. Proofs are not reusable across sessions and are discarded promptly.

The protocol is transport-independent and may run after iroh, BLE, Wi-Fi, or another authenticated peer connection is established.

## Relay behaviour

The Festival DO may retain opaque encrypted group state/log payloads for catch-up. It must enforce payload size, topic count, rate, mailbox quota, and retention without requiring the group key.

The DO must not infer group membership from ambient discovery or expose subscriber lists to clients.

## Current implementation evidence

The repository contains:

- group key generation/derivation and AES-GCM helpers;
- local group persistence;
- Yrs group documents with nested members, stars, and pins;
- check-in, star, pin, create/join/leave methods;
- group topic derivation;
- resource registry and sync orchestrator;
- group sync/private-handshake foundations;
- chat persistence and HWM structures;
- Flutter create/join/invite/social screens;
- compact encrypted Meshtastic group-chat debug send/apply.

The remaining work is integration and end-to-end proof, not recreation of these components.

## Acceptance criteria

### Lifecycle

- Create and join register both resources before returning success to the normal UI flow.
- Local group state remains visible after restart with no network.
- Subscriptions and catch-up resume after app/route restart.

### Convergence

- Two offline members independently mutate metadata, check-ins, stars, and pins, then converge without lost unrelated fields.
- Bilateral state-vector exchange handles each peer having unique changes.
- Duplicate/reordered updates produce one converged state.

### Privacy

- The Durable Object and unrelated peers receive no plaintext group data.
- Private discovery reveals no raw group IDs, keys, or reusable membership tokens.
- A wrong key cannot apply state or chat.

### Chat

- Normal Social UI messages persist, broadcast, decrypt, and notify on another member's device.
- Bounded HWM catch-up fills gaps idempotently after reconnect.
- Clock skew does not produce inconsistent ordering.
- Meshtastic applies the same logical message once and never carries bulk history.

### Platform flow

- QR display and scanning use the versioned real payload.
- Deep links and repeated scans are idempotent.
- At least two devices pass create/join/sync/restart tests on each available route profile.

## Deferred

- Group-key rotation and cryptographic eviction.
- Public nearby-person discovery outside shared groups.
- Server-visible group moderation or recovery.
- Peer bootstrap of unknown festivals.
