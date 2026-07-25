# Auth & Identity Protocol

> **Status: proposed.** Passkey/session behaviour and group-key possession are the intended model. The exact offline MainDO-attestation policy for public chat remains an explicit decision in `execution-plan.md`.

This document describes the identity, authentication, and message-signing
protocol proposed across Offbeat transports. It supersedes the "Future
work" section of `admin-protocol.md` where the implementation adopts it.

---

## Design Goals

1. **One-time setup** — register once on first app launch, never again
2. **No anonymous writes** — every public write is attributable to a
   WebAuthn-registered identity
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
authority. The accepted offline policy must define how missing/expired
attestations affect trust display and relay behaviour. The 64-byte
signature cannot be meaningfully truncated.

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

3. Issue a signed **attestation** — proof that this Ed25519 public key
   was registered via WebAuthn:

   ```
   MainDO signs: "attestation:v1:<pubkey_hex>:<issued_at_unix>:<expires_at_unix>"
   ```

   The attestation is a portable certificate. The client stores it
   locally and presents it when connecting to any Festival DO or peer.

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

When connecting to a Festival DO (or a P2P peer), the client
authenticates once per session. After that, writes are permitted without
further checks.

### Relay (Festival DO WebSocket)

The existing WS protocol adds an `auth` message type:

```json
// Client → DO (immediately after connect, before any writes)
{
  "type": "auth",
  "publicKey": "<64-char hex Ed25519 public key>",
  "attestation": "<MainDO-signed attestation>",
  "signature": "<hex Ed25519 sig over the string 'session:<timestamp>'>"
}
```

The DO:

1. Verifies the attestation signature against MainDO's well-known
   public key
2. Checks expiry (with grace period)
3. Verifies the session signature proves ownership of the public key
4. Sets `session.authenticated = true` and `session.publicKey = ...`

**After auth:**

- `subscribe`, `catchup` — always allowed (no auth needed)
- `gossip` (write) — requires `session.authenticated == true`

Unauthenticated clients can read freely. Writes without auth return:

```json
{ "type": "error", "message": "auth required for writes" }
```

**Per-write overhead: one boolean check.** No crypto on the hot path.

### P2P (iroh-gossip)

Peers exchange attestations during the gossip topic join handshake:

1. Peer A connects to peer B on a shared topic
2. Both send their attestation + a signed challenge
3. Each side verifies the other's attestation
4. Peers without valid attestations can receive messages but their
   published messages are silently dropped by receivers

This is symmetric — both sides authenticate to each other.

---

## 3. Per-Message Signing (Public Traffic Only)

Public writes (stage chat, festival updates) include the sender's full
Ed25519 public key and a 64-byte signature. Group traffic is **not**
signed — see Trust Layers above.

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

#### Tier 2: Fully self-contained (160B overhead)

```
┌─────────────┬──────────────────┬───────────────┬────────────────┐
│ sender_key  │ payload          │ sender_sig    │ attestation_sig│
│ (32 bytes)  │ (variable)       │ (64 bytes)    │ (64 bytes)     │
└─────────────┴──────────────────┴───────────────┴────────────────┘
```

Includes the MainDO attestation signature alongside the message
signature. Any receiver can verify both authorship AND registration
without any prior state or DO connectivity. The receiver verifies
`attestation_sig` against MainDO's well-known public key over the
attestation message `attestation:v1:<sender_key_hex>:<issued>:<expires>`.

- **Pros**: fully offline-verifiable, no trust state needed
- **Cons**: 160B overhead per message (80% on a 200B payload)
- **Best for**: single messages in low-trust environments

#### Tier 3: Batched attestations (amortized overhead)

```
┌─ Metapacket ──────────────────────────────────────────┐
│ Attestation table (header):                            │
│ ┌─────────────────────────────────────────────────┐   │
│ │ [0] key=<pubkey_A> att_sig=<attestation_sig_A>  │   │
│ │ [1] key=<pubkey_D> att_sig=<attestation_sig_D>  │   │
│ └─────────────────────────────────────────────────┘   │
│                                                        │
│ Messages:                                              │
│ ┌──────────┬──────────────────┬────────────┐          │
│ │ att_idx=0│ payload          │ sender_sig │ (from A) │
│ │ att_idx=1│ payload          │ sender_sig │ (from D) │
│ │ att_idx=0│ payload          │ sender_sig │ (from A) │
│ └──────────┴──────────────────┴────────────┘          │
└────────────────────────────────────────────────────────┘
```

A forwarding peer bundles all messages it is relaying into a
metapacket. The header contains the attestation table — one entry
per unique sender — and each message references its sender by index
(1 byte). The attestation cost (64B per unique sender) is paid once
per sender per batch, not per message.

Per-message overhead: **65B** (1B index + 64B signature).
Per-sender overhead: **96B** (32B key + 64B attestation sig), once.

For a batch of 20 messages from 5 senders:

- Tier 1: 20 × 96B = 1,920B
- Tier 2: 20 × 160B = 3,200B
- Tier 3: 5 × 96B + 20 × 65B = 480B + 1,300B = **1,780B**

Tier 3 is cheaper than Tier 1 AND provides full registration proof.
The savings grow with batch size and sender repetition.

- **Pros**: amortized attestation cost, fully offline-verifiable,
  most efficient for relay/forwarding scenarios
- **Cons**: more complex framing, requires batching
- **Best for**: P2P relay and mesh forwarding where a peer forwards
  messages from multiple senders

### Tier selection by context

| Context | Tier | Why |
|---|---|---|
| Relay (DO) | 1 | DO already verified the session |
| Direct P2P (first msg from sender) | 2 | Receiver has no prior state |
| Direct P2P (subsequent) | 1 | Receiver cached the attestation |
| P2P relay / mesh forwarding | 3 | Amortize across forwarded messages |
| LoRa (single message) | 1 or 2 | Depends on payload budget |

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

On the relay path, both authorship and registration are verified
immediately — the DO only relays from authenticated sessions. On P2P,
trust depends on the tier:

- **Tier 2/3**: registration is verifiable immediately (attestation
  included). Accept if valid, drop if expired/forged.
- **Tier 1**: registration is deferred. Check `known_keys` table;
  if unknown, accept tentatively and display as "unverified."

```
Receive signed message
        │
        ├─ Signature invalid or missing → drop, never relay
        │
        ├─ Attestation included (tier 2/3)?
        │       ├─ Valid → accept as trusted, cache in known_keys
        │       └─ Invalid/expired → drop
        │
        ├─ No attestation (tier 1), key in known_keys → accept (trusted)
        │
        └─ No attestation, key unknown → accept AND relay
                │                         display as "unverified" in UI
                │
                └─ On next DO connection:
                    ├─ Key confirmed → promote, add to known_keys
                    └─ Key not registered → prune messages locally
```

Signed messages from unknown keys **are relayed freely**. This is
essential for WiFi-direct meshes and P2P networks to function without
DO access. A group of peers at a stage with no internet must be able
to chat — blocking relay of unknown keys would break this.

Spam from throwaway keys is mitigated by **rate limiting per
connection**, not by blocking relay. On DO reconnect, messages from
unattested keys are pruned locally.

### Known keys and friends

Each device maintains two local tables for identity resolution:

```sql
CREATE TABLE known_keys (
    public_key TEXT PRIMARY KEY,
    display_name TEXT,         -- last seen self-asserted name
    verified_at TEXT,
    source TEXT NOT NULL       -- 'group', 'attestation', 'do_confirmed'
);

CREATE TABLE friends (
    public_key TEXT PRIMARY KEY,
    name TEXT NOT NULL,        -- user's chosen name for this person
    added_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

`known_keys` is populated automatically from:

- **Group membership**: when joining a group, all member keys are added
  (source: `group`)
- **Direct attestation exchange**: P2P handshake or tier 2/3 messages
  (source: `attestation`)
- **DO confirmation**: batch validation on reconnect (source:
  `do_confirmed`)

`friends` is populated manually by the user.

### Display name resolution

Names in chat messages are **self-asserted** — the sender includes
whatever display name they want in the payload. The public key (bound
by the signature) is the true identity. Names are resolved locally:

```
1. friends table   → your name for them    (highest priority)
2. known_keys table → last seen name       (verified sender)
3. message payload → self-asserted name    (unverified indicator)
```

If a known key starts using a different name, the `known_keys` table
updates silently. If a friend changes their self-asserted name, the
friend name you set still takes priority.

### Verification code

Public messages — verify signature, check trust:

```rust
fn handle_public_message(msg: &WireMessage, db: &Database) -> TrustLevel {
    // Step 1: verify authorship
    if !signing::verify(&msg.sender_key, &msg.payload, &msg.signature) {
        return TrustLevel::Invalid; // drop
    }

    // Step 2: check attestation if present (tier 2/3)
    if let Some(att_sig) = &msg.attestation_sig {
        if verify_attestation(&msg.sender_key, att_sig, &MAIN_DO_PUBKEY) {
            db.add_known_key(&msg.sender_key, "attestation");
            return TrustLevel::Trusted;
        } else {
            return TrustLevel::Invalid; // forged attestation
        }
    }

    // Step 3: check local trust (tier 1)
    if db.is_known_key(&msg.sender_key) {
        TrustLevel::Trusted
    } else {
        db.track_unverified_key(&msg.sender_key);
        TrustLevel::Unverified // display with indicator, relay freely
    }
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
| Festival update | ~100B | 96B (key+sig) | **196B** |
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

| Transport | Session auth | Public msgs | Group msgs | What syncs |
|---|---|---|---|---|
| Internet (relay) | WS `auth` message | Ed25519 (96B) | GCM only | Everything |
| WiFi Direct | Attestation exchange | Ed25519 (96B) | GCM only | Everything |
| BLE | Attestation exchange | Ed25519 (96B) | GCM only | Group state, group chat (last 50) |
| LoRa/Meshtastic | Attestation exchange | Ed25519 (96B) | GCM only | Group state, check-ins, chat |

### LoRa payload budget

Meshtastic packets have ~228B MTU. Group messages (the primary LoRa use
case) have no signature overhead — just GCM's 28B (nonce + tag), leaving
~200B for payload. This is enough for check-ins, short chat, and small
CRDT updates.

Public messages over LoRa pay the full 96B auth overhead (32B pubkey +
64B sig), leaving ~132B for payload. For fragmented messages, the auth
data covers the reassembled payload — paid once, not per fragment.

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

Attestations expire after 30 days (+7 day grace). Users who don't
refresh are gradually excluded from writing. This handles abandoned
devices and natural churn.

### Active revocation

MainDO maintains a revocation list:

```sql
CREATE TABLE revocations (
    public_key TEXT PRIMARY KEY,
    revoked_at TEXT NOT NULL,
    reason TEXT
);
```

Festival DOs sync the revocation list periodically (on WS connect, same
pattern as admin sync). Revoked keys are rejected even if their
attestation hasn't expired.

For P2P networks without relay access, revocation propagates through
peers who have recently connected to a DO. Peers gossip revocation
lists alongside attestations.

---

## 9. MainDO Public Key Distribution

Peers need MainDO's public key to verify attestations offline. This key
is distributed through:

1. **Hardcoded in the app binary** — the primary trust anchor
2. **`GET /public-key`** on MainDO — for key rotation
3. **Peer exchange** — peers share the key during attestation handshake

Key rotation: MainDO can rotate its signing key. Old attestations remain
valid until expiry. New attestations use the new key. The app ships with
both old and new keys during the transition window.

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
  |                        |-- auth { attestation, sig } ----------------->|
  |                        |<-- auth_ok { adminCount: 0 } ----------------|
  |                        |                        |                     |
  |  "become admin?" ----->|                        |                     |
  |  "yes"                 |-- PUT /admins -------------------------------->|
  |                        |<-- 200 ok -----------------------------------|
  |                        |                        |                     |
  |  (send stage chat)     |                        |                     |
  |                        |-- gossip { payload, sig } ------------------>|
  |                        |                 verify session → relay to all |
  |                        |                        |                     |
  |  (send group chat)     |                        |                     |
  |                        |-- gossip { GCM ciphertext } ---------------->|
  |                        |                        store + relay (opaque) |
  |                        |                        |                     |
Peer (public)              |                        |                     |
  |-- WS connect + auth ------------------------------------------------->|
  |<-- gossip { sender_id, payload, sig } --------------------------------|
  |  (verify sig, accept)  |                        |                     |
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
