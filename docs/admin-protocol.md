# Admin & Festival Management Protocol

This document describes the authentication, authorization, and festival
lifecycle protocol used by the offbeat server. All admin operations use
Ed25519 key pairs for identity and request signing.

> **See also:** [`auth-protocol.md`](auth-protocol.md) — WebAuthn
> registration, attestation, session auth, and per-message signing.
> Admin identity is built on top of the verified identity described there.

---

## Overview

```
                    MainDO (singleton)
                    +--------------------------+
                    | admins table             |
                    | festivals / lineup data  |
                    +-----------+--------------+
                                |
                     sync on WS connect
                                |
                    +-----------v--------------+
                    | FestivalDO (per-festival) |
                    | admins table (inherited)  |
                    | Ed25519 keypair           |
                    | gossip_log                |
                    +--------------------------+
```

Two levels of admin:

- **Global admins** (MainDO) -- can create festivals, manage lineup data
- **Festival admins** (FestivalDO) -- can export signing keys, push signed
  updates. Global admins are automatically synced here.

---

## 1. Ed25519 Identity

Every admin is identified by a 32-byte Ed25519 verifying (public) key,
represented as a 64-character hex string.

On the mobile app, this key will be derived from a WebAuthn passkey.
During development, generate a keypair with
`ed25519_dalek::SigningKey::generate()` (Rust) or
`ed25519.keygen()` (@noble/curves, TypeScript).

---

## 2. Request Authentication

Admin-protected endpoints on the MainDO use two headers:

| Header | Value |
|--------|-------|
| `X-Admin-Key` | 64-char hex Ed25519 public key |
| `X-Admin-Sig` | Hex-encoded Ed25519 signature |

**What gets signed:** the **URL pathname** of the request, encoded as
UTF-8 bytes. For example, a `PUT` to `/festivals/fieldday26/lineup`
requires a signature over the bytes of the string
`/festivals/fieldday26/lineup`.

Festival DO endpoints use **fixed challenge strings** instead of the
path (see section 5).

**Verification flow:**

1. Look up `X-Admin-Key` in the `admins` table
2. If not found: **403 Forbidden**
3. Verify the signature: `ed25519.verify(sig, message, pubkey)`
4. If invalid: **401 Unauthorized**

---

## 3. Bootstrapping the First Admin

The first admin is auto-accepted (no signature required). Subsequent
admins require an authenticated request from an existing admin.

### Register global admin

```
PUT /admins
Content-Type: application/json

{ "publicKey": "<64 hex chars>" }
```

- **First call** (empty admins table): accepted unconditionally.
- **Subsequent calls**: requires `X-Admin-Key` header (existing admin)
  and body `signature` field -- a signature over `"add-admin:<newPublicKey>"`.

**Response:** `{ "ok": true }` (200)

### List global admins

```
GET /admins
```

**Response:** `["<pubkey hex>", ...]` (200)

### Register festival-specific admin

```
PUT /festivals/:id/admins
Content-Type: application/json

{ "publicKey": "<64 hex chars>" }
```

Same bootstrap logic. Typically not needed because global admins are
synced automatically (see section 7).

---

## 4. Promoting Other Admins

Once bootstrapped, an existing admin promotes another by signing the
message `add-admin:<newPublicKeyHex>` and including the result:

```
PUT /admins
Content-Type: application/json
X-Admin-Key: <existing admin pubkey hex>

{
  "publicKey": "<new admin pubkey hex>",
  "signature": "<hex sig over 'add-admin:<new admin pubkey hex>'>"
}
```

The same pattern works on `PUT /festivals/:id/admins` for
festival-level promotion.

---

## 5. Creating a Festival

### From direct metadata

```
POST /festivals
Content-Type: application/json
X-Admin-Key: <pubkey hex>
X-Admin-Sig: <hex sig over "/festivals">

{
  "id": "glastonbury26",
  "name": "Glastonbury 2026",
  "year": 2026,
  "location": "Worthy Farm",
  "city": "Pilton",
  "country": "GB",
  "startDate": "2026-06-24",
  "endDate": "2026-06-28",
  "genres": ["Rock", "Electronic", "World"],
  "status": "upcoming"
}
```

**Response:** `201 Created` with the festival object.

### Registered-user Clashfinder authoring

Festival discovery exposes an inline **ADD CLASHFINDER** flow to every currently
registered user. It is separate from administrator mutation endpoints:

```text
POST /festival-imports/preview
POST /festival-imports/:previewId/publish
```

Preview accepts a public Clashfinder URL or short ID, fetches it with server-only
credentials, validates bounded schedule data, and stores an owner-bound preview
for 15 minutes. Publish confirms name, venue, city, and two-letter country code,
then atomically creates the server-authoritative registry entry and lineup. The
API idempotently seeds signed Festival DO state and returns an existing festival
for duplicate sources or fresh signed retries. Requests use the registered-user
signature contract in `auth-protocol.md` and are limited per user, network, and
globally.

### Administrator Clashfinder source

```
POST /festivals
Content-Type: application/json
X-Admin-Key: <pubkey hex>
X-Admin-Sig: <hex sig over "/festivals">

{
  "source": {
    "festivalId": "fieldday26",
    "clashfinderId": "fieldday2026",
    "name": "Field Day 2026",
    "location": "Victoria Park, London",
    "city": "London",
    "country": "GB",
    "genres": ["Electronic", "Indie"]
  }
}
```

The server fetches the lineup from the Clashfinder API (using stored
`CLASHFINDER_USERNAME` / `CLASHFINDER_PRIVATE_KEY` secrets), parses it,
and stores both the festival metadata and the full lineup.

**Response:** `201 Created` with `{ festival, lineup }`.

### Updating metadata

```
PUT /festivals/:id
Content-Type: application/json
X-Admin-Key: <pubkey hex>
X-Admin-Sig: <hex sig over "/festivals/:id">

{ "name": "New Name", "city": "Berlin" }
```

Only provided fields are updated.

### Publishing / replacing a lineup

```
PUT /festivals/:id/lineup
Content-Type: application/json
X-Admin-Key: <pubkey hex>
X-Admin-Sig: <hex sig over "/festivals/:id/lineup">

{
  "events": [
    { "artist": "The Cure", "stage": "Main Stage", "day": "friday",
      "start": "21:00", "end": "23:00" },
    ...
  ]
}
```

Alternatively, pass a pre-parsed `"lineup"` object instead of `"events"`.
Replaces the entire lineup (stages, days, sets).

---

## 6. Festival Authority Key

Each Festival DO generates a stable Ed25519 authority keypair on first boot. The public key is available to clients and must be pinned through trusted festival metadata:

```
GET /festivals/:id/public-key
```

**Response:** 64-character hex public key (plain text).

The current protocol treats the Festival DO as the sequence authority. Administrative clients authenticate mutation requests; they do not send `FestivalUpdate` envelopes through gossip. The legacy `POST /festivals/:id/signing-key` export endpoint still exists, but it is not part of the supported sync path: an offline signer cannot safely allocate the DO's monotonic authority sequence or update its canonical checkpoint. Do not build new clients around exported authority secrets.

---

## 7. Submitting Authoritative Updates

An admin sends a Yrs mutation to the Festival DO:

```
POST /festivals/:id/sign-update
Content-Type: application/json

{
  "publicKey": "<admin pubkey hex>",
  "signature": "<hex sig over 'sign-update:<docId>'>",
  "docId": "festival/<id>/state",
  "topic": "festival/<id>/state",
  "update": "<base64 Yrs update bytes>"
}
```

The DO:

1. verifies the admin identity and requires `docId` to equal the festival state topic;
2. applies the Yrs mutation to its canonical festival document;
3. allocates the next monotonic `authoritySeq`;
4. signs a live delta and the resulting full checkpoint;
5. persists both verified envelopes, retaining the latest checkpoint and bounded recent deltas;
6. broadcasts the signed delta to subscribers; and
7. returns `{ docId, kind, authoritySeq, signedUpdate, publicKey }`.

The signature covers the protocol domain separator, document ID, update kind (`DELTA = 1`, `CHECKPOINT = 2`), authority sequence, and Yrs update bytes. Client-authored `FestivalUpdate` gossip messages are rejected by the Festival DO rather than relayed or stored.

---

## 8. Global Admin Sync

Global admins registered on the MainDO are automatically pushed to each
Festival DO on the first WebSocket connection:

1. Client connects to `GET /festivals/:id/ws`
2. API layer calls `syncAdminsToFestival()`
3. Fetches `GET /admins` from MainDO
4. Calls `FestivalDO.importAdmins(keys)` (RPC)
5. Festival DO runs `INSERT OR IGNORE` for each key

This is idempotent and runs on every WS connect, so new global admins
propagate automatically.

---

## 9. Verification on the Client

When a client receives a protobuf `FestivalUpdate` through the DO, gossip, or direct peer catch-up, it:

1. resolves the pinned festival authority public key;
2. requires a known update kind and a positive, non-rollback authority sequence;
3. reconstructs the canonical signed bytes from the domain separator, document ID, kind, sequence, and update bytes;
4. verifies the Ed25519 signature before Yrs apply;
5. applies and persists the verified envelope idempotently; and
6. retains verified checkpoints so it can later serve the same signed authority envelope to a peer.

An unknown authority, malformed envelope, invalid signature, rollback, or legacy unsigned `svDiff` is rejected. A peer may replay a verified envelope, but it may never generate a trusted state-vector diff itself. The DO also rejects client-authored festival envelopes; clients still verify independently because relays are not trust roots.

---

## Error Codes

| Code | Meaning |
|------|---------|
| 200 | Success |
| 201 | Created |
| 400 | Missing or invalid fields |
| 401 | Missing auth headers, or signature verification failed |
| 403 | Public key not in admins table |
| 404 | Festival not found |
| 409 | Festival already exists |
| 500 | Server error (keypair not initialized, missing secrets) |

---

## Sequence: Full Festival Lifecycle

```
Admin                    MainDO                  FestivalDO
  |                        |                        |
  |-- PUT /admins -------->|  (bootstrap)           |
  |<-- 200 ok ------------|                         |
  |                        |                         |
  |-- POST /festivals ---->|  (create + lineup)     |
  |<-- 201 created --------|                         |
  |                        |                         |
  |-- WS /festivals/x/ws --------------------------->|
  |                        |-- syncAdmins RPC ------>|
  |                        |                         |
  |-- POST signing-key --------------------------->|
  |<-- 200 secret key --------------------------------|
  |                        |                         |
  |  (sign update locally)                           |
  |                        |                         |
  |-- gossip festival_update ---------------------->|
  |                        |            store + relay |
  |                        |                         |
Peer                       |                         |
  |-- WS connect ------------------------------------->|
  |-- catchup sinceSeq:0 ------------------------------>|
  |<-- catchup [festival_update] -----------------------|
  |  (verify sig, apply)   |                         |
```
