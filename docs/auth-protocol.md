# Auth & Identity Protocol

> **Status: accepted trust policy.** Public-chat authorship, offline attestation, and unknown-sender behaviour are defined below. Delivery implementation is tracked by `offbeat-t6a.4`; causal ordering is tracked separately by `offbeat-t6a.11`.

This document describes the identity, authentication, and message-signing
protocol proposed across Offbeat transports. It supersedes the "Future
work" section of `admin-protocol.md` where the implementation adopts it.

---

## Design Goals

1. **One-time setup** — register once on first app launch, never again
2. **No anonymous relay writes** — the Festival DO accepts public writes only
   from an attested session; disconnected peers may tentatively carry a signed
   but unverified message under the bounded policy below
3. **Reads are open** — anyone can subscribe and receive data
4. **Group trust = key possession** — all group traffic (chat, CRDTs,
   check-ins) is authenticated by AES-256-GCM encryption alone. If you
   have the group key, you're trusted. No per-message signatures.
5. **Public-chat signing** — attendee Ed25519 signatures prove authorship
   for public stage/general/campsite messages
6. **Festival authority** — festival state has a separate organiser/DO
   signing key and is never authorised by an attendee attestation
7. **Session-level access control** — expensive auth checks happen once
   per connection, not per message

---

## Trust Layers

Three trust domains have different guarantees:

### Public attendee chat

| Layer | Question it answers | Cost | Where checked |
|-------|---------------------|------|---------------|
| WebAuthn/MainDO attestation | "Was this key registered?" | Once per attestation lifetime | MainDO and peers with trusted MainDO keys |
| Session authentication | "Can this connection write?" | Once per session | DO or peer handshake |
| Ed25519 message signature | "Which key authored this message?" | Per message | Every receiver |

A valid Ed25519 signature proves authorship, not registration or organiser
authority. Missing or out-of-grace attestations produce an explicitly
unverified message; they are not equivalent to invalid signatures or forged
attestations. The 64-byte signature cannot be meaningfully truncated.

### Festival state

| Layer | Question it answers | Where checked |
|-------|---------------------|---------------|
| Festival authority certificate/key | "Which key may update this festival?" | Trusted festival metadata |
| Ed25519 update signature | "Did that authority author these bytes?" | Every receiver before apply |

Attendee identity and MainDO registration never authorise a lineup,
cancellation, or festival announcement. Relays may transport signed
festival state but cannot create it.

### Group traffic (chat, CRDTs, check-ins, presence)

| Layer | Question it answers | Cost | Where checked |
|-------|---------------------|------|---------------|
| Group key (AES-256-GCM) | "Is this person in our group?" | Shared once on join | Every receiver |

Group members are implicitly trusted once they possess the group key.
The GCM authentication tag (16 bytes, inherent in every encrypted
message) proves the sender has the key. No Ed25519 signatures are
used for group traffic — the key is the credential.

This means group members can impersonate each other within the group.
For groups of ~5 friends at a festival, this is an acceptable trade-off
that eliminates 68 bytes of overhead per message and simplifies the
protocol significantly, especially on constrained transports (BLE, LoRa).

---

## Accepted Offline Public-Chat Policy

The accepted model is **signed authorship with cached registration proof and
bounded deferred trust**. It applies only to public attendee chat.

### Trust states

| Evidence | Apply and display | Forward | Reconciliation |
|---|---|---|---|
| Missing/invalid attendee signature | Drop | Never | Record only a bounded diagnostic |
| Valid signature + current MainDO attestation | Verified | Yes | Cache the attestation |
| Valid signature + attestation expired by at most 7 days | Verified offline | Yes | Refresh when any full route returns |
| Valid signature + no usable attestation | Unverified | Yes, within the unverified quota | Request proof on a capable route |
| Cryptographically valid attestation beyond grace | Unverified | Yes, within the unverified quota | Treat like missing proof; expiry is not forgery |
| Forged/malformed attestation or known-revoked key | Drop | Never | Persist a bounded rejection marker |

An author cannot evade expiry handling by omitting an old attestation: missing
proof and valid-but-out-of-grace proof have the same unverified state. A peer
must never infer organiser authority from any attendee trust state.

### Accepted transport representation

Every public-chat message carries a 32-byte writer public key and an untruncated
64-byte Ed25519 signature over canonical domain-separated message bytes. Full
and low-bandwidth routes exchange a separate compact MainDO attestation on
session establishment or cache miss; they do not repeat it on every message.
The canonical attestation bytes are `offbeat/attestation/v1`, issuer key ID,
issuer generation, writer public key, issued-at Unix seconds, and expires-at Unix
seconds, encoded with fixed-width integers and length-delimited binary fields.
The proof carries those fields plus the issuer's 64-byte signature. Verification
selects the public key only from the locally trusted keyset by signed issuer ID;
a proof-supplied issuer key is never trusted. Binary protocol fields are required
on the wire rather than the current JSON/hex storage form.

This baseline deliberately does not implement attestation metapackets or
per-message attestation duplication. They remain optional optimisations only if
measurement demonstrates a need.

### Trust-anchor rotation

Normal MainDO key rotation uses canonical `offbeat/main-keyset/v1` metadata that
contains a monotonically increasing generation, validity window, each key's ID
and public key, activation time, issuance cutoff, and verification end. It is
signed by the previously trusted active key and the new key. Clients persist the
highest accepted generation and reject rollback, unknown chains, invalid
cross-signatures, overlapping key IDs, and attestations issued by an old key
after its cutoff. An old attestation remains valid only when its issue time is at
or before that cutoff and its expiry is at or before the old key's verification
end.

Compromise of the currently trusted key cannot be repaired by metadata signed
only by that key. Emergency distrust therefore requires a denylist/keyset shipped
through an authenticated app update rooted outside MainDO. Mixed-version peers
may transport newer metadata but cannot make another peer trust it without that
peer's existing chain or app root.

| Route profile | Message auth overhead | Registration proof behaviour |
|---|---:|---|
| Full | 96 bytes plus framing | Exchange compact proof on session/cache miss |
| Low bandwidth | 96 bytes plus framing | Exchange compact proof on cache miss, subject to route cap |
| Constrained | 96 bytes plus compact framing | Never bulk-transfer proofs or history; unknown writers remain unverified until another route supplies proof |

A constrained public message that does not fit the measured route payload is
suppressed rather than weakening or truncating its signature. Festival and
group traffic retain their own trust envelopes.

### Relay, peer, and UI behaviour

- The Festival DO requires a current or grace-period attested session, requires
  the message writer key to equal the session key, verifies the per-message
  signature, and applies topic/payload/rate limits before persistence.
- Clients verify every public message independently; successful relay delivery
  is not proof of authorship or registration.
- Full/low-bandwidth peers request a missing attestation by writer key. A proof
  is accepted only under a pinned MainDO trust anchor.
- Unverified messages are stored with an `unverified` trust state, show an
  **UNVERIFIED** badge and self-asserted name, and cannot populate trusted-name
  state. They are capped per writer and topic, expire from the visible chat
  after 24 hours unless promoted, and are not eligible for historical catch-up.
- The initial abuse-control defaults are a burst of 5 and 30 messages per
  minute per writer/connection, at most 20 visible unverified messages per
  writer, 100 visible unverified messages per topic, and 100 admitted
  unverified writer keys per topic. Implementations may tighten these limits by
  route but may not make them unbounded.
- Each admitted unverified writer retains a persistent per-topic sequence floor
  until that festival's cached data is explicitly deleted. A first live message
  establishes the floor because unverified traffic is ineligible for historical
  catch-up; later expired or quota-evicted messages advance it. Sparse accepted,
  rejected, and equivocated tuples are retained for 30 days. Thus visibility
  expiry, quota eviction, restart, or route replay cannot resurrect a message.
  Once the 100-writer admission table is full, new unverified writers are
  rejected rather than evicting replay state.
- On reconnect, positive MainDO validation promotes matching messages. A
  definitive unregistered or revoked result hides them and retains the writer
  floor and rejection state. Network failure is not a negative validation
  result.

### Authenticated revocation propagation

Revocations are irreversible within a MainDO trust generation. MainDO emits
`offbeat/revocations/v1` signed full snapshots and hash-linked deltas containing
trust generation, monotonic revocation generation, issue time, previous
generation/hash, and entries of writer key, revocation time, and reason code.
Every full snapshot is cumulative for that trust generation.

Clients verify objects against the accepted keyset and persist the highest
contiguous generation plus the union of revoked keys. A delta applies only when
its previous generation/hash exactly matches local state. A gap or out-of-order
delta does not advance the generation; it triggers retrieval of the latest full
snapshot. A newer valid cumulative snapshot may bridge the gap. No older,
incomplete, or reordered object can remove a persisted revocation.

A single object is capped at 256 KiB and 4096 entries; larger snapshots use
signed, hash-linked pages under one generation and apply atomically only after
all pages and the root hash verify. Forged, conflicting, rollback,
unknown-chain, incomplete, or oversized data is rejected without changing
trust. Peers may transport these signed objects but never author them. Snapshot
age may prevent a negative "not revoked" conclusion, but a valid signed
revocation remains applicable offline.

Expiry and visibility decisions use a persisted trust-clock floor that never
moves backwards across restart: the maximum of local time, previously observed
trusted server time, and previously persisted trust time. Clock rollback cannot
extend an attestation or revive a hidden message; clock advance can only
downgrade a message to unverified, not forge or revoke it.

### Signed bytes and replay handling

The signature covers a protocol/version domain, festival and chat topic,
message ID, writer key, writer sequence, causal-order value, display timestamp,
display name, and text. Re-encoding or moving a signed message to another topic
therefore invalidates it. Wall time remains display-only.

An exact duplicate is ignored by stable message ID. Different signed payloads
using the same topic, writer key, and writer sequence are writer equivocation.
The convergent result is to quarantine and hide **all** variants for that tuple,
persist an equivocation marker, and mark the sequence consumed.

The chat state vector carries each writer's HWM **and the message ID at that
HWM**, plus bounded equivocated sequence markers retained for 30 days. Equal
sequences with different IDs force exchange of both signed envelopes regardless
of HWM. Any receiver of two valid distinct variants emits an
`EquivocationProof` containing those envelopes. Messages stop forwarding, but
the bounded proof continues over normal capable catch-up; constrained routes
carry only the compact marker and defer proof retrieval. A marker affects trust
only after local proof verification. Once verified, all peers advertise the
same `EQUIVOCATED` commitment for that sequence and quarantine any current or
later-arriving variant.

`offbeat-t6a.11` defines the remaining causal-order value and database order; it
may not weaken this commitment exchange, proof verification, quarantine, or
deduplication contract.

### Required evidence before public chat ships

1. Invalid signatures, forged attestations, revoked writers, cross-topic
   replay, and exact duplicates are rejected; two partitions that each receive
   a different same-sequence variant exchange HWM commitments/proof and converge
   to quarantine across restart, forwarding, and catch-up.
2. Current, grace-period, absent, expired, and later-promoted/rejected
   attestations produce the defined persistent trust and UI states.
3. Relay ingress rejects unpinned issuers and replayed/expired/wrong-festival
   session challenges, proves session-key/message-key equality, and enforces
   payload, topic, and rate limits.
4. Unverified quotas, writer admission, sequence floors, and 24-hour visibility
   expiry survive restart, eviction, and multiple-route replay without message
   resurrection.
5. Normal key overlap/retirement and emergency distrust reject rollback,
   unknown chains, post-cutoff issuance, invalid cross-signatures, and stale
   generation after restart.
6. Forged, stale, rollback, conflicting, oversized, and paginated revocation
   data cannot incorrectly add or remove trust; skipped/out-of-order deltas do
   not advance state and recover through a cumulative signed snapshot.
7. Full and low-bandwidth cache-miss proof exchange works offline; constrained
   routes send no proofs or history and suppress oversize public messages.
8. `pnpm check`, `pnpm check:rust`, Flutter analysis/tests, and targeted server
   trust tests pass.

---

## 1. Identity Setup (First App Launch)

On first launch, the app performs a one-time WebAuthn passkey
registration. The Ed25519 identity key is **deterministically derived**
from the passkey using the WebAuthn PRF extension — your identity IS
your biometric. This is the only time the user interacts with auth.

### PRF key derivation

The WebAuthn PRF extension evaluates a pseudo-random function bound to
the credential's internal secret. Given a salt, it produces a
deterministic 32-byte output that only the authenticator can compute.

```
PRF(credential_secret, "offbeat-ed25519-v1") → 32 bytes
    → HKDF-SHA256(ikm=prf_output, salt="offbeat", info="ed25519-identity")
    → 32-byte Ed25519 seed
    → Ed25519 keypair
```

Same passkey + same salt = same Ed25519 key, every time, on any device.
If you lose your phone, authenticate with the same passkey on a new
device and your identity is recovered automatically.

**Platform support:** Android (Google Password Manager), iOS 18.4+
(iCloud Keychain), macOS 15+ (Safari 18+).

### Flow

```
App                              MainDO
 |                                  |
 |-- POST /auth/register/begin --->|
 |<-- challenge + options ---------|
 |                                  |
 |  (platform authenticator        |
 |   creates passkey)              |
 |                                  |
 |  (authenticate with PRF salt    |
 |   to derive Ed25519 keypair)    |
 |                                  |
 |-- POST /auth/register/complete ->|
 |   { webauthnResponse,           |
 |     ed25519PublicKey }           |
 |<-- { attestation } -------------|
 |                                  |
 |  (store key + attestation)      |
```

### What happens server-side

1. `POST /auth/register/begin` — generate a WebAuthn registration
   challenge. The relying party is `offbeat.app` (or `localhost` in dev).
   Request PRF extension support in the options.

2. `POST /auth/register/complete` — verify the attestation response.
   The client also sends the Ed25519 public key it derived from PRF.
   Store in the `credentials` table:

   ```sql
   INSERT INTO credentials (id, user_id, public_key, credential_data, created_at)
   VALUES (?, ?, ?, ?, datetime('now'));
   ```

   The `public_key` column stores the Ed25519 public key (hex).

3. Issue the accepted compact **attestation** proving that this Ed25519
   public key was registered via WebAuthn. MainDO signs the canonical
   `offbeat/attestation/v1` fields defined above, including issuer key ID and
   generation, writer key, issue time, and expiry.

   The attestation is a portable certificate. The client stores it locally and
   presents it to a Festival DO or a capable peer on session/cache miss.

> **Note:** The server cannot verify that the Ed25519 key was actually
> derived from PRF — the PRF output is only visible client-side. The
> server trusts the client's claim. If a client lies and uses a random
> key instead, they only lose deterministic recovery on device change.
> The server's security model is unaffected.

### What happens client-side (Flutter + Rust)

1. Flutter performs WebAuthn registration via `passkeys` package
2. Flutter authenticates with PRF salt `"offbeat-ed25519-v1"` to get
   the 32-byte PRF output
3. PRF output is passed to Rust via bridge
4. Rust runs HKDF and derives the Ed25519 keypair
5. Keypair is stored in the credentials table
6. Attestation from MainDO is stored alongside

```sql
-- credentials table (existing key-value store)
key = "identity_secret_key"   → 32-byte Ed25519 seed (from PRF + HKDF)
key = "attestation"           → MainDO-signed attestation blob
key = "attestation_expires"   → Unix timestamp
```

### Device recovery

On a new device, the user authenticates with their existing passkey
(biometric). The PRF extension produces the same 32-byte output, which
derives the same Ed25519 keypair. The identity is recovered without
any server-side key storage or transfer.

```
New device                       MainDO
 |                                  |
 |-- POST /auth/recover/begin ---->|
 |<-- challenge + options ---------|
 |                                  |
 |  (authenticate with passkey,    |
 |   PRF derives same Ed25519 key) |
 |                                  |
 |-- POST /auth/recover/complete -->|
 |   { assertion, ed25519PublicKey } |
 |<-- { attestation (renewed) } ---|
 |                                  |
 |  (same identity, new device)    |
```

The server verifies the WebAuthn assertion, confirms the Ed25519 public
key matches the stored one, and issues a fresh attestation.

### Attestation expiry and refresh

Attestations expire after **30 days**. The app silently refreshes when
it has connectivity by re-authenticating with the stored passkey:

```
App                              MainDO
 |                                  |
 |-- POST /auth/refresh ---------->|
 |   { credentialId,               |
 |     assertion (WebAuthn) }      |
 |<-- { attestation (renewed) } ---|
```

On pure-P2P networks without connectivity, expired attestations are
still accepted by peers with a **7-day grace period**. This ensures
a festival-length offline window doesn't break writes.

---

## 2. Session Authentication

When connecting to a Festival DO (or a P2P peer), the client authenticates once
per session. Session authentication establishes registration and key possession;
every public message still undergoes signature, key-binding, topic, payload,
rate, replay, and trust-state checks before persistence or forwarding.

### Relay (Festival DO WebSocket)

The production WS protocol uses a server-generated, single-use challenge rather
than accepting a client timestamp as replay protection:

```json
// Festival DO → client
{ "type": "auth_challenge", "nonce": "<random 32 bytes>", "festivalId": "<id>" }

// Client → Festival DO, before any writes
{
  "type": "auth",
  "publicKey": "<32-byte Ed25519 public key>",
  "attestation": "<compact MainDO-signed attestation>",
  "signature": "<Ed25519 signature over offbeat/session/v1, festivalId, nonce>"
}
```

The DO:

1. Verifies the attestation issuer against a pinned MainDO trust anchor, then
   verifies the proof signature and writer-key binding.
2. Checks expiry with the 7-day grace period and checks known revocation state.
3. Atomically consumes the challenge and verifies the domain/festival-bound
   session signature, proving possession of the attested key.
4. Sets `session.authenticated = true` and `session.publicKey = ...`.

A challenge expires after five minutes, is scoped to one socket/festival, and
cannot be reused after success or failure.

**After auth:**

- `subscribe`, `catchup` — always allowed (no auth needed)
- `gossip` (write) — requires `session.authenticated == true`

Unauthenticated clients can read freely. Writes without auth return:

```json
{ "type": "error", "message": "auth required for writes" }
```

Attestation verification is session-level. Public-chat writes still require a
per-message signature check and equality between the message writer key and the
authenticated session key; opaque group writes require normal size/topic/rate
checks.

### P2P (iroh-gossip)

On full and low-bandwidth routes, peers exchange attestations during the topic
join handshake or on cache miss:

1. Peer A connects to peer B on a shared topic.
2. Both send a signed challenge and any requested compact attestation.
3. Each side verifies the challenge and proof against a pinned MainDO key.
4. Missing or out-of-grace proof produces the bounded unverified state; an
   invalid signature, forged proof, or known revocation is dropped.

This is symmetric. Constrained routes carry authorship-only live messages and
rely on a later capable route to promote unknown writers.

---

## 3. Per-Message Signing (Public Traffic Only)

Public attendee chat includes the sender's full Ed25519 public key and a
64-byte signature. Festival updates instead use the distinct festival-authority
envelope; group traffic is **not** signed — see Trust Layers above.

### Signing tiers

Three tiers of per-message auth, offering different tradeoffs between
overhead and offline trust guarantees:

#### Tier 1: Authorship only (96B overhead)

```
┌─────────────┬──────────────────┬───────────────┐
│ sender_key  │ payload          │ signature     │
│ (32 bytes)  │ (variable)       │ (64 bytes)    │
└─────────────┴──────────────────┴───────────────┘
```

Proves "key X sent this." Does **not** prove X is WebAuthn-registered.
Registration verification is deferred until the receiver can check
with the DO or has seen X's attestation through another channel.

- **Pros**: compact, self-contained for authorship
- **Cons**: receivers cannot distinguish registered from unregistered
  senders without additional state
- **Best for**: relay path (DO already verified the session), low-
  bandwidth transports

#### Alternatives not selected for the baseline

A self-contained message could repeat the attestation signature, issue/expiry
times, and issuer identifier. That adds at least 80 bytes beyond the 96-byte
writer key/signature and repeats unchanged proof on every message. A batched
metapacket could amortize proofs across writers, but introduces another framing
and buffering protocol.

Neither representation is selected initially. Offbeat exchanges one compact
proof sidecar per writer and attestation lifetime on capable routes. Revisit
per-message or batched proof carriage only with measured cache-miss or framing
evidence.

### Accepted tier selection

The implementation baseline uses Tier 1 messages on every route and exchanges
a compact attestation separately on capable routes. The self-contained and
batched alternatives above are retained for comparison, not requirements.

| Context | Accepted representation | Why |
|---|---|---|
| Relay (DO) | Tier 1 message + attested session; proof sidecar on receiver cache miss | Avoid repeated proof while preserving independent client verification |
| Direct full/low-bandwidth P2P | Tier 1 message + proof at handshake/cache miss | Cache once per writer and attestation lifetime |
| P2P forwarding | Preserve the original signed message; forward proof separately when requested | No new metapacket format without measured need |
| Constrained | Tier 1 compact live message only | Never transfer proofs or history; defer registration trust |

### Wire format (group messages)

```
┌──────────────────────────────────────────────┐
│ AES-256-GCM ciphertext                       │
│ (12B nonce ‖ encrypted payload ‖ 16B tag)    │
└──────────────────────────────────────────────┘
```

No sender key or signature. The sender self-identifies within the
encrypted payload (e.g. display name, user ID). Group members trust
this self-identification because possessing the group key implies
the group vouched for the sender.

### Verification and deferred trust

A valid Ed25519 signature proves **authorship** — not registration.
An attacker can trivially generate an Ed25519 keypair. The signature
prevents impersonation of a specific key but not spam from
unregistered keys.

On the relay path, both authorship and registration are verified immediately —
the DO requires an authenticated session and verifies each public-message
signature. On P2P, trust depends on the available evidence:

- A separately supplied current or grace-period proof promotes the writer.
- Missing or out-of-grace proof is deferred trust: accept tentatively under the
  quota and display as **UNVERIFIED**.
- A forged/malformed proof, invalid message signature, or known revocation is
  rejected. Expiry alone is not forgery.

```
Receive signed message
        │
        ├─ Signature invalid/missing, proof forged, or key revoked
        │       └─ drop, reject replay, never relay
        │
        ├─ Cached/supplied proof current or within grace
        │       └─ accept as verified and cache proof
        │
        └─ Proof missing or beyond grace
                └─ accept as unverified only within quota
                        ├─ later confirmed → promote
                        ├─ definitively rejected/revoked → hide + reject replay
                        └─ network unavailable → remain unverified
```

Signed messages from unknown keys are relayed only within the accepted
per-writer/topic quotas. This keeps disconnected peer chat functional without
turning deferred trust into an unbounded spam path. On reconnect, positive
validation promotes messages; definitive unregistered/revoked results hide
them and retain replay-resistant rejection markers.

### Known keys and friends

Each device persists registration evidence independently from local identity
labels. The implementation schema must represent at least:

- writer public key;
- when a proof exists, all compact attestation fields and its issuer key ID;
- issue/expiry timestamps and last successful validation;
- `verified`, `verified_offline`, `unverified`, or `revoked` status;
- bounded rejected message IDs/writer-sequence tuples.

Group membership, a self-asserted display name, or a locally assigned friend
name may improve identity display but **must not** promote registration trust.
Only a MainDO proof rooted in a pinned key or a positive authenticated MainDO
validation can do that.

### Display name resolution

Names in chat messages are **self-asserted** — the sender includes
whatever display name they want in the payload. The public key (bound
by the signature) is the true identity. Names are resolved locally:

```
1. local friend label   → your name for them    (highest display priority)
2. verified profile    → last seen verified name
3. message payload     → self-asserted name
```

Name resolution never changes the message's registration badge. A friend label
may replace the displayed name, but an unverified writer remains visibly
**UNVERIFIED** until MainDO evidence promotes that key.

### Verification code

Public messages — verify signature, check trust:

```rust
fn classify_public_message(msg: &WireMessage, evidence: Evidence) -> TrustLevel {
    if !verify_domain_separated_message(msg)
        || evidence.is_forged()
        || evidence.is_revoked()
    {
        return TrustLevel::Invalid;
    }
    if evidence.is_current() {
        return TrustLevel::Verified;
    }
    if evidence.is_within_grace() {
        return TrustLevel::VerifiedOffline;
    }
    TrustLevel::Unverified // persist/forward only if bounded quotas allow it
}
```

Group messages — decrypt (GCM tag verification is implicit):

```rust
fn verify_group_message(msg: &[u8], group_key: &[u8; 32]) -> Option<Vec<u8>> {
    crypto::decrypt(group_key, msg).ok()
}
```

---

## 4. Data Volume Analysis

### Assumptions (72-hour festival)

- 10 stages, 1000 subscribers each
- ~1 msg/min per stage chat (public)
- Groups of 5: 5 check-ins, 10 messages, 5 star updates per user
- Festival: 3 announcements, 5 set updates (~100B each)

### Per-message costs

| Message type | Payload | Auth overhead | Total |
|---|---|---|---|
| **Public** | | | |
| Stage chat | ~100B | 96B (key+sig) | **196B** |
| Festival update | ~100B | Separate authority envelope | Transport-dependent |
| **Group** (encrypted, no sig) | | | |
| Group chat | ~100B | 28B (GCM) | **128B** |
| Group star update | ~30B | 28B (GCM) | **58B** |
| Group check-in | ~20B | 28B (GCM) | **48B** |

### System-wide totals (72 hours)

**Stage chat (dominant cost):**

- 1 msg/min × 60 min × 15 hrs/day × 3 days × 10 stages = 27,000 msgs
- Per message fanned out to 1000 subscribers = 27M deliveries
- DO egress: 27,000 × 196B × 1,000 = **5.1GB**
- Auth portion of egress: 27,000 × 96B × 1,000 = **2.5GB** (49%)

**Per-user ingress (subscribed to 2 stages at a time):**

- ~4.4MB over 72 hours — negligible

**Group traffic (per group of 5):**

- ~7KB total over 72 hours — negligible
- No signature overhead — just GCM encryption

### Overhead verdict

49% of stage chat egress is auth data (pubkey + signature). In absolute
terms, 5.1GB over 72 hours from a single DO is within Cloudflare's
capabilities and costs roughly $0.25 in egress. The overhead is
measurably present but not practically painful. The full public key
is included (vs a truncated ID) so that messages are self-verifiable
without prior key exchange — essential for the deferred trust model
on P2P.

Group traffic is tiny regardless and benefits most from dropping
signatures — especially on constrained transports where every byte
matters. A group chat message is 128B instead of 224B (43% smaller).

---

## 5. Transport Considerations

What varies across transports is the session auth mechanism and what
data types are worth syncing. Signing rules are determined by traffic
type (public vs group), not transport.

| Transport | Session/proof exchange | Public msgs | Group msgs | What syncs |
|---|---|---|---|---|
| Internet (relay) | WS auth + proof on cache miss | Ed25519 (96B) | GCM only | Everything |
| WiFi Direct/Aware | Attestation exchange | Ed25519 (96B) | GCM only | Everything |
| BLE | Bounded attestation exchange | Ed25519 (96B) | GCM only | Bounded state/chat |
| LoRa/Meshtastic | No proof/history transfer | Ed25519 (96B), live and size-gated | GCM only | Compact eligible operations |

### LoRa payload budget

Meshtastic packets have ~228B MTU. Group messages (the primary LoRa use
case) have no signature overhead — just GCM's 28B (nonce + tag), leaving
~200B for payload. This is enough for check-ins, short chat, and small
CRDT updates.

Public messages over LoRa pay the full 96B auth overhead (32B pubkey +
64B sig), leaving ~132B before Offbeat/Meshtastic framing. Existing bounded
fragmentation may carry one eligible compact message, with the signature paid
once over the logical payload; messages beyond the constrained route cap are
suppressed rather than weakening authentication.

---

## 6. Group Traffic Topology

Group gossip is scoped to group members only. Non-members never relay
group traffic:

```
DO (reliable delivery, when online)
 ↕
Group members ←→ Group members (direct P2P, any transport)
```

- The DO stores group messages for catchup
- P2P connections between group members use any available transport
- No stranger relaying — write amplification equals group size (N=5)
- Mesh discovery between group members is direct (e.g. two members
  with Meshtastic nodes discover each other and link nearby members)

### Eviction

Removing a member from the group CRDT (the members list) causes other
group members to stop accepting messages from them. However, this is
a **social/application-level** exclusion, not a cryptographic one:

- The evicted member still possesses the group key
- A compromised or malicious client can continue to decrypt and listen
  to group traffic indefinitely
- We do **not** implement forward secrecy or key rotation on eviction
- We do **not** derive a new key for remaining participants

This is an intentional trade-off. For groups of ~5 friends at a
festival, key rotation adds significant complexity (re-keying all
members, handling offline members during rotation, split-brain states)
for a marginal threat scenario. The group key is shared in person (QR
code) and the social trust model assumes members don't become
adversaries mid-festival.

If a group is truly compromised, the remaining members should create
a new group and share a new key.

---

## 7. First Admin Bootstrap

When a user is the first to connect to a new Festival DO, the app
prompts them:

> "This festival has no admin yet. Would you like to become the admin?"

If accepted:

1. The app calls `PUT /festivals/:id/admins` with the user's Ed25519
   public key
2. Since the admins table is empty, the first registration is accepted
   unconditionally (existing bootstrap logic, see `admin-protocol.md`)
3. The user becomes the festival admin and can manage lineup data,
   export signing keys, etc.

This flow requires that the user has already completed WebAuthn
registration (section 1). The admin role is layered on top of the
verified identity.

### Detection

On WS connect, after the `auth` handshake, the DO responds with:

```json
{
  "type": "auth_ok",
  "authenticated": true,
  "adminCount": 0
}
```

The client uses `adminCount: 0` to trigger the admin prompt.

---

## 8. Revocation

### Passive expiry

Attestations expire after 30 days with a 7-day offline grace period. Beyond
grace, the Festival DO excludes the key from relay writes. A disconnected peer
may carry its correctly signed messages only as bounded unverified traffic until
the proof refreshes; expiry never grants verified status.

### Active revocation

MainDO stores irreversible writer revocations and publishes the bounded,
signed, monotonic snapshots/deltas defined under **Authenticated revocation
propagation**. Festival DOs refresh them periodically; peers may relay the same
verified objects. No peer-authored, stale, rollback, conflicting, or oversized
object changes trust. A persisted valid revocation overrides an otherwise valid
attestation and is never cleared by omission from an older snapshot.

---

## 9. MainDO Public Key Distribution

Peers need MainDO's public key to verify attestations offline. Trust anchors
come from the app binary and authenticated MainDO key-rotation metadata. A peer
may advertise a key identifier or supply a missing certificate, but peer
exchange alone can never establish a new trust anchor.

Key rotation: MainDO can rotate its signing key. Old attestations remain valid
until expiry. New attestations use the new key. The app ships with both old and
new keys during the transition window, or accepts a new key only through a
signature chain rooted in an already trusted key.

---

## Sequence: Full Identity Lifecycle

```
User                     App (Rust)               MainDO              FestivalDO
  |                        |                        |                     |
  |  (first launch)        |                        |                     |
  |  "set up passkey" ---->|                        |                     |
  |                        |-- register/begin ----->|                     |
  |  (biometric/PIN) ----->|-- register/complete -->|                     |
  |                        |<-- attestation --------|                     |
  |                        |  (store locally)       |                     |
  |                        |                        |                     |
  |  (open festival)       |                        |                     |
  |                        |-- WS connect -------------------------------->|
  |                        |<-- auth_challenge { nonce, festivalId } ------|
  |                        |-- auth { attestation, challenge_sig } ------->|
  |                        |<-- auth_ok { adminCount: 0 } ----------------|
  |                        |                        |                     |
  |  "become admin?" ----->|                        |                     |
  |  "yes"                 |-- PUT /admins -------------------------------->|
  |                        |<-- 200 ok -----------------------------------|
  |                        |                        |                     |
  |  (send stage chat)     |                        |                     |
  |                        |-- gossip { writer, payload, sig } ---------->|
  |                        |   verify session/writer equality, signature, |
  |                        |   topic, payload, replay, and rate → persist |
  |                        |   and relay                                  |
  |                        |                        |                     |
  |  (send group chat)     |                        |                     |
  |                        |-- gossip { GCM ciphertext } ---------------->|
  |                        |                        store + relay (opaque) |
  |                        |                        |                     |
Peer (public)              |                        |                     |
  |-- WS challenge/auth ------------------------------------------------>|
  |<-- gossip { writer, payload, sig } -----------------------------------|
  |  verify signature/topic/replay; classify cached proof                 |
  |  → verified, verified-offline, unverified-within-quota, or drop       |
  |                        |                        |                     |
Peer (group member)        |                        |                     |
  |<-- gossip { GCM ciphertext } -----------------------------------------|
  |  (decrypt with group key, accept)               |                     |
```

---

## Error Codes (Auth-specific)

| Code | Context | Meaning |
|------|---------|---------|
| 200 | `auth_ok` | Session authenticated |
| 400 | `auth` | Missing or malformed fields |
| 401 | `auth` | Attestation signature invalid, or session sig invalid |
| 403 | `gossip` | Write attempted without prior auth |
| 410 | `auth` | Attestation expired (beyond grace period) |
| 423 | `auth` | Public key revoked |
