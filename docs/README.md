# Offbeat documentation

This directory contains product requirements, protocol specifications, architecture guidance, and historical implementation plans.

## Source-of-truth order

When documents disagree, use this order:

1. `CLAUDE.md` — repository structure, commands, invariants, and current platform stack.
2. `docs/prd.md` — authoritative product behaviour and acceptance criteria.
3. Protocol documents — security and wire-level rules for their named boundary.
4. `docs/sync-patterns.md` — current resource and transport-profile semantics.
5. `docs/execution-plan.md` — current sequencing and validation plan.
6. Historical PRDs and execution ledgers — design context, not instructions to recreate completed work.
7. Source code and tests — evidence of what is implemented today.

Implementation work is tracked in the local Beads workspace at `../beads/offbeat`. Beads records the active dependency graph; repository documents explain durable product and architecture decisions.

## Current documents

| Document | Status | Purpose |
|---|---|---|
| `prd.md` | Authoritative | Product scope, user stories, functional requirements, and success metrics |
| `execution-plan.md` | Active | Remaining implementation sequence and validation gates |
| `sync-patterns.md` | Active | Resource semantics, transport profiles, prioritisation, catch-up, and route promotion |
| `meshtastic-implementation-plan.md` | Active | Meshtastic protocol, platform bridge, and field-test accountability |
| `auth-protocol.md` | Proposed; decision pending | Passkey, attestation, public authorship, and group trust model |
| `admin-protocol.md` | Active | Administrative signing and festival authority operations |
| `prd-group-pubsub.md` | Active gap specification | Group registration, mutation broadcast, private discovery, and invite flows |
| `prd-p2p-direct-connectivity.md` | Design history/current requirements | Stable identity, peer discovery, and direct-route bootstrap |
| `reactive-resource-layer-prd.md` | Implemented foundation with open transport work | Resource registry, state-vector/HWM sync, reactive streams, and transport boundary |
| `reactive-resource-layer-execution-plan.md` | Completion ledger | What the reactive-resource phases delivered and what remains |
| `gaps.md` | Active | Verified remaining correctness, resilience, and integration gaps |
| `prd.json`, `reactive-resource-layer-prd.json` | Historical snapshots | Machine-readable snapshots; Markdown documents take precedence |

## Settled boundaries

- The mobile client is Flutter with a Rust core exposed through flutter_rust_bridge. Tauri/Svelte instructions are obsolete.
- The top-level festival/event registry is server-authoritative. Clients cache successful discovery responses for offline browsing; peers do not introduce previously unseen events.
- Festival lineup and announcements do not use REST as a mobile data path. They flow through signed festival state in the Rust sync layer and persist locally.
- Syncable domain data is represented as four logical resources: `FestivalState`, `GroupState`, `StageChat`, and `GroupChat`.
- CRDT documents catch up with Yrs state vectors. Append logs catch up with per-writer high-water marks.
- Group state and group chat are encrypted with the group key. Festival state is signed by the festival authority. Public-chat authorship has a separate trust policy.
- Meshtastic uses official Meshtastic protobuf envelopes and Offbeat-owned `PRIVATE_APP` payload bytes. Bulk CRDT snapshots and chat history are suppressed on constrained routes.
- Generated FRB files are regenerated, never edited by hand.

## Open architecture decisions

Two boundaries deliberately remain undecided until prototypes provide evidence:

1. **All-iroh transport scope:** BLE has a credible existing iroh custom transport. The Durable Object WebSocket and Meshtastic route require measured feasibility before deciding whether they carry native iroh framing or adapt the shared resource protocol.
2. **Offline public-chat trust:** Ed25519 proves authorship, while MainDO attestations prove registration. The final offline verification and expiry policy remains to be selected.

These decisions must not fork the domain model: resource schemas, deduplication IDs, privacy rules, and convergence semantics remain transport-agnostic.

## Documentation maintenance

- Update durable behaviour here when an implementation decision is accepted.
- Record work state and blockers in Beads rather than adding unchecked task lists to PRDs.
- Do not paste agent prompts or commit instructions into specifications.
- Mark historical assumptions explicitly instead of leaving contradictory directions in active documents.
- Prefer links to a single authoritative explanation over copied protocol pseudocode.
