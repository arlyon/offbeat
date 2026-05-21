# Admin & Festival Management Protocol

This document describes the authentication, authorization, and festival
lifecycle protocol used by the offbeat server. All admin operations use
Ed25519 key pairs for identity and request signing.

> **Future work:** WebAuthn passkey registration will derive the Ed25519
> public key used here, replacing manual key management on the mobile app.

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

### From Clashfinder source

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

## 6. Exporting the Festival Signing Key

Each Festival DO generates a stable Ed25519 keypair on first boot. The
public key is available to anyone:

```
GET /festivals/:id/public-key
```

**Response:** 64-char hex public key (plain text).

An admin can export the **secret** key to sign updates offline:

```
POST /festivals/:id/signing-key
Content-Type: application/json

{
  "publicKey": "<admin pubkey hex>",
  "signature": "<hex sig over the string 'export-signing-key'>"
}
```

**Response:** 64-char hex secret key (plain text, 200).

The exported key allows an admin to sign updates locally and gossip them
through any channel (WS relay, P2P, BLE). Any peer can verify against
the public key from `GET /public-key`.

---

## 7. Submitting Signed Updates

### Option A: Admin signs locally (offline-capable)

1. Export the signing key (section 6)
2. Create a Yrs CRDT update
3. Sign the raw update bytes with the exported key
4. Construct a `festival_update` gossip wire message:
   ```json
   {
     "kind": "festival_update",
     "doc_id": "festival/<id>",
     "payload": "<JSON string of SignedUpdate>",
     "group_key_id": null
   }
   ```
   where `SignedUpdate` is:
   ```json
   {
     "update": "<base64 Yrs update bytes>",
     "author": "festival-organiser",
     "signature": "<base64 Ed25519 signature>"
   }
   ```
5. Send via WebSocket gossip:
   ```json
   { "type": "gossip", "topic": "festival/<id>/state", "message": <wire> }
   ```

Peers verify the signature against the DO's public key before applying.

### Option B: DO signs on behalf of admin (online)

```
POST /festivals/:id/sign-update
Content-Type: application/json

{
  "publicKey": "<admin pubkey hex>",
  "signature": "<hex sig over 'sign-update:<docId>'>",
  "docId": "festival/<id>",
  "topic": "festival/<id>/state",
  "update": "<base64 Yrs update bytes>"
}
```

The DO:
1. Verifies admin identity
2. Signs the update with its own key
3. Stores in `gossip_log` (assigned a sequence number)
4. Broadcasts to all connected WS subscribers
5. Returns `{ seq, signedUpdate, publicKey }`

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

When a client receives a `festival_update` message (via gossip or
catchup), it:

1. Fetches the festival's public key (`GET /public-key` or cached)
2. Extracts the `SignedUpdate` from the wire message payload
3. Base64-decodes the `update` and `signature` fields
4. Calls `ed25519.verify(publicKey, updateBytes, signatureBytes)`
5. If valid: applies the Yrs update to the local document
6. If invalid: discards silently (the DO is a dumb relay)

The DO does **not** verify signatures on gossip messages it relays. It
stores and broadcasts everything. Verification is always client-side.

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
