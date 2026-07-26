# Sync patterns

> Authoritative resource, catch-up, prioritisation, and transport-profile semantics for Offbeat.

## Principle: one logical model, adaptive physical routes

Offbeat synchronises four logical resources. Their IDs, privacy boundaries, deduplication rules, and apply semantics do not change with the transport.

| Resource | Data shape | Visibility | Catch-up |
|---|---|---|---|
| `FestivalState` | Yrs CRDT | Public, signed by festival authority | Signed full checkpoint plus signed live deltas |
| `GroupState` | Yrs CRDT | AES-256-GCM with group key | Encrypted state-vector diff |
| `StageChat` | Append log | Public, signed by message author | Per-writer high-water mark |
| `GroupChat` | Append log | AES-256-GCM with group key | Per-writer high-water mark |

Physical routes advertise a profile. The profile controls batching, history limits, representation, and whether a resource is suppressed. It must not introduce a second domain model.

## Route profiles

| Profile | Typical routes | Intended traffic |
|---|---|---|
| `Full` | Internet, LAN, Wi-Fi Aware, Wi-Fi Direct | State-vector diffs, bounded chat catch-up, live updates |
| `LowBandwidth` | BLE | Bounded state diffs, small recent chat windows, live high-priority updates |
| `Constrained` | Meshtastic/LoRa | Compact absolute operations, no snapshots, no history |

Route classification is based on measured capability, not merely a transport name. A poor BLE link may become `Constrained`; a high-quality local IP path is `Full`.

### Wi-Fi peer routes

Wi-Fi Aware/NAN is the preferred no-access-point high-bandwidth route where hardware and firmware expose it. Android support is runtime-gated by `FEATURE_WIFI_AWARE`; capable silicon is insufficient if an OEM disables the service. iOS supports app-to-app Wi-Fi Aware only on recent systems and supported hardware.

Wi-Fi Direct is a possible coverage fallback, not a separate sync protocol. Either route may feed native IP hints into iroh or use a custom transport adapter when the platform exposes connection objects rather than sockets.

## Priority classes

| Priority | Resources | Behaviour |
|---|---|---|
| P0 critical | Festival cancellations, lineup changes, urgent announcements | Send first; retain longest; compact on constrained links |
| P1 high | Group metadata, membership, check-ins, stars, pins | Send after P0; compact on constrained links |
| P2 normal | Group chat | Live and bounded on capable routes; short compact messages on constrained links |
| P3 low | Public festival chat, historical data | Interest-filtered; idle-only live frames on constrained links; never bulk history |

Priority affects scheduling, not trust. A P0 update is still rejected if its festival-authority signature is invalid.

## CRDT catch-up

### Full and low-bandwidth routes

Group state uses bilateral state-vector exchange:

1. Each peer sends its state vector for the resource.
2. Each peer computes the update missing from the remote vector.
3. Group vectors and diffs are encrypted with the group key.
4. Each peer decrypts, applies, and persists the received update idempotently.
5. Resource watchers emit the new typed snapshot.

State-vector exchange is bilateral because both group peers may contain unique offline changes.

Festival state is single-authority and therefore uses a stricter catch-up path:

1. The Festival DO applies each authenticated administrative mutation to its canonical Yrs document.
2. It emits an authority-signed delta and persists an authority-signed full checkpoint at the same monotonic authority sequence.
3. A late client requests catch-up through the normal route; the DO or peer returns its latest persisted signed checkpoint, never a peer-synthesised Yrs diff.
4. The client verifies the signature over the protocol domain, document ID, update kind, authority sequence, and Yrs bytes before applying.
5. The client rejects unknown authorities, invalid kinds, zero sequences, and sequence rollback, then persists the verified envelope for restart and peer relay.

A Yrs state vector may still be sent as the catch-up request hint, but it never authorises an unsigned `svDiff`. Only the festival authority can create a festival delta or checkpoint that another client will apply.

### Constrained routes

Do not send a Yrs snapshot or arbitrary diff over Meshtastic. Map eligible mutations to compact absolute operations, for example:

- festival set moved/cancelled;
- urgent announcement;
- member checked in to stage/custom location;
- member shared/unshared one set;
- pin added/removed.

Each operation carries a stable resource ID, operation/message ID, freshness/order metadata, and the normal trust envelope. Applying it invokes the same domain mutation and watcher path as another route.

Compact operations are optimisation representations, not a separate source of truth. A later state-vector sync reconciles any missed history.

## Append-log catch-up

Each writer has a stable public writer key and monotonic sequence. A chat state
vector maps writer keys to the highest contiguous sequence and its message ID.
It also carries bounded equivocation markers. Equal writer sequences with
different IDs are therefore detectable rather than hidden by identical HWMs.

Catch-up flow:

1. Exchange per-writer high-water marks, head commitments, and bounded
   equivocation markers for the topic.
2. Return messages newer than the remote mark, capped by the route profile. On
   a same-sequence commitment mismatch, exchange both signed variants despite
   the HWM; a verified conflict becomes a bounded `EquivocationProof` that
   capable routes continue forwarding until peers advertise `EQUIVOCATED`.
3. Verify/decrypt before persistence. An unproven peer marker never changes
   trust; a verified proof quarantines every variant and consumes the sequence.
4. Insert ordinary messages by stable message ID with `INSERT OR IGNORE`
   semantics.
5. Notify watchers once for the applied batch.

Authoritative ordering must not rely only on device wall clocks. Use a deterministic causal tuple, such as hybrid logical time, writer key, and writer sequence. Wall time remains display metadata.

### History limits

- `Full`: bounded recent peer/relay catch-up; older public history may be paginated online.
- `LowBandwidth`: a small recent window only.
- `Constrained`: no catch-up/history request. Only new eligible compact messages are sent.

## Topic interest

Subscription is explicit and persistent:

- opening a festival subscribes to its state resource;
- joining/creating a group immediately registers and subscribes to group state and group chat;
- checking in subscribes to the current stage chat;
- manually selected stage chats remain subscribed until the user removes them;
- unsubscribed public topics do not consume live bandwidth;
- historical public chat loads on navigation rather than global subscription.

Interest filtering applies before low-priority scheduling.

## Group discovery and privacy

Peers may discover that they share a group without revealing the group ID, group key, or membership list to unrelated peers. The private handshake uses possession-derived tokens with a fresh session nonce. After a match, peers register the shared resources and perform normal bilateral catch-up.

The Durable Object stores group updates and chat only as opaque encrypted blobs. Possession of the group key is the current membership credential; key rotation remains a future capability.

Personal stars remain canonical in local SQLite. Group membership continuously
mirrors those same-festival stars into the member's per-set entries in every
matching encrypted `GroupState`; create, join, and restart subscription
reconcile missed changes. Leaving removes access and stops future sharing.
Schedule overlays are derived locally from cached lineup metadata plus the
converged group documents, never synced as a separate resource. Every local
group-state delta is also persisted as an encrypted, festival-scoped outbound
intent before publication. Relay failures retry with bounded backoff and survive
restart; rows clear only after the relay echoes the durably sequenced envelope.
Exact retries reuse their original server sequence. Leave atomically compacts
older outbound deltas into its encrypted wire envelope while deleting the local
group key, chat history, and cached plaintext group document. Moving between
festivals closes and awaits the previous relay loop before opening another.
Relay catch-up filters the global resource registry by the active festival, and
outbound festival/group chat verifies the relay scope before publication. Logs
report lane/count diagnostics without exposing private group topic identifiers.

## Public trust boundaries

Different public data has different authority:

- `FestivalState`: only the configured festival authority may author updates.
- `StageChat`: an untruncated, domain-separated attendee signature proves
  message authorship, never organiser authority.
- A MainDO attestation rooted in a pinned key proves registration for 30 days,
  with a 7-day offline grace period.

The accepted public-chat policy is cached registration proof with bounded
deferred trust. A valid signature with current/grace proof is verified; a valid
signature with missing or out-of-grace proof is stored and forwarded only within
unverified quotas, visibly badged, excluded from history catch-up, and eligible
for later promotion or rejection. Invalid signatures, forged proofs, known
revocations and cross-topic replay are dropped. Writer-sequence equivocation
quarantines every variant and marks the tuple consumed so opposite delivery
orders converge without stalling high-water-mark catch-up.
Full and low-bandwidth routes exchange compact proof sidecars on cache miss.
Constrained routes carry only size-gated live authorship envelopes, never proofs
or history.

Relays are untrusted delivery infrastructure. A Festival DO requires an
attested session, enforces equality between session and writer keys, verifies
public-message signatures, and rate-limits ingress. Clients still independently
verify every message before apply. See `auth-protocol.md` for the state table,
quotas, UI, reconciliation, and validation contract.

## Event discovery boundary

The top-level festival registry is not a peer-synchronised resource in the current product scope. It is fetched from the MainDO and cached locally for offline browsing. A fresh install without network cannot learn a never-seen event from peers.

Once a festival is known and opened, its signed festival state may synchronise through peers without REST lineup fetching.

## Local persistence and retry

The durable order for a local mutation is:

1. apply to the local resource;
2. persist state/message and outbound intent atomically where possible;
3. notify the local UI;
4. enqueue for eligible active routes;
5. mark delivery progress without deleting the source mutation prematurely.

Retries use stable operation/message IDs and are idempotent. Queue entries have deliberate expiry based on priority and semantics. Route loss or app termination must not discard accepted writes.

SQLite work that can block is isolated from async network reactors. Catch-up batches use transactions rather than one commit per message.

## Deduplication

Deduplicate at two layers:

- transport framing: fragment/message IDs prevent repeated reassembly;
- resource application: Yrs update idempotence or stable append-log message IDs prevent repeated domain effects.

Receiving the same logical update over WebSocket, iroh, BLE, Wi-Fi, and Meshtastic must produce one persisted effect and one coherent watcher update.

## Route promotion handshake

When two peers meet, exchange enough capability information to select an efficient route without exposing unrelated private resources:

- protocol version;
- stable endpoint identity;
- supported route types and profiles;
- festival/public topic interests;
- privacy-preserving shared-group discovery material;
- resource summaries needed to start SV/HWM exchange.

A constrained encounter may advertise a better route. If both peers can establish BLE, Wi-Fi Aware/Direct, LAN, or internet connectivity, promote bulk catch-up to that route and keep the constrained route for urgent fallback.

## Meshtastic

Meshtastic owns the BLE/radio protobuf envelope. Offbeat owns `Data.payload` for `PortNum::PrivateApp`.

The production baseline uses compact, prioritised resource frames with fragmentation/reassembly and deduplication. Whether native iroh framing is feasible remains an evidence-driven architecture decision. Any prototype must measure actual byte and airtime cost and must not re-enable bulk snapshots/history on LoRa.

See `meshtastic-implementation-plan.md` for UUIDs, phases, and hardware tests.

## Safe Yrs lifecycle

Persist valid Yrs updates or full encoded state for reload. Do not “compact” by replacing a live document with a fresh document created from its current snapshot without proving that future updates from peers with older state vectors still merge correctly.

Compaction requires a protocol-aware design with migration and convergence tests. Until then, correctness takes priority over shrinking local blobs.

## Failure behaviour

- Invalid signatures: drop and record a bounded diagnostic.
- Unknown festival authority: do not apply; request trusted metadata when online.
- Group decrypt failure: drop without revealing key-dependent detail.
- Malformed/oversized frame: reject before allocation or persistence.
- Queue full: preserve higher priority entries; expose degraded status.
- Peer/route loss: retry through another eligible route.
- Catch-up interruption: resume from persisted SV/HWM state.
- No route: remain fully usable from local state and show honest transport status.

## Validation matrix

Every resource should be tested across stable seams:

| Test | Required evidence |
|---|---|
| Local mutation | Persisted before network and visible through normal watcher |
| Duplicate/reordered delivery | One final effect and deterministic state |
| Partition convergence | Both peers independently mutate, reconnect, and converge |
| Restart | State, writer sequence, and outbound intent survive |
| Trust failure | Forged signature/wrong key is rejected |
| Profile suppression | Low/constrained route does not send forbidden bulk data |
| Route switch | Same logical update deduplicates across routes |
| Multi-device | Real hardware confirms discovery, connection, and propagation |

Performance targets and command-level validation live in `execution-plan.md`.
