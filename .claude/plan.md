# Plan: Forward admin lineup sync to Festival DO via Yrs

## Problem

When an admin calls `PUT /festivals/:id/lineup` to refresh a schedule from
Clashfinder, the new lineup is stored in **MainDO's SQL tables only**. The
Festival DO — which holds the Yrs CRDT document that clients actually sync
from — is never notified. Connected clients don't see the update until the
Festival DO is reset and re-seeded.

## Current Flow

```
Admin → PUT /festivals/:id/lineup → MainDO
  1. Fetch from Clashfinder API
  2. Parse into Lineup (stages/days/sets)
  3. Upsert into MainDO SQL tables
  4. Return JSON
  ❌ Festival DO is NOT updated
```

## Proposed Flow

```
Admin → PUT /festivals/:id/lineup → MainDO
  1. Fetch from Clashfinder API
  2. Parse into Lineup (stages/days/sets)
  3. Upsert into MainDO SQL tables
  4. Return JSON

→ api.ts route handler (after MainDO responds)
  5. Forward the new lineup to the Festival DO
  6. Festival DO applies it as an incremental Yrs update
  7. Festival DO signs, stores in gossip_log, broadcasts to WS clients
```

## Changes

### 1. `FestivalDO.updateLineup()` — new RPC method (`festival-do.ts`)

Add a new method similar to `seedLineup()` but designed for **updates** rather
than genesis:

- Loads the existing Yrs doc (from `yrs_docs` + replaying `gossip_log`)
- Captures the state vector **before** mutation
- Overwrites the `stages`, `days`, `sets` keys on the root map with the new
  JSON values
- Encodes an **incremental** Yrs update (diff from previous state vector)
- Signs the update with the DO's Ed25519 key
- Stores as a `FestivalUpdate` `GossipEnvelope` in `gossip_log`
- Persists consolidated doc to `yrs_docs`
- Broadcasts to all subscribed WS clients

This follows the exact same pattern as `#writeWeatherToDoc()` which already
does load → capture SV → mutate → diff → sign → store → broadcast.

### 2. `PUT /festivals/:id/lineup` route in `api.ts`

After forwarding to MainDO and getting a successful response:

- Parse the lineup from the MainDO response body
- Get the Festival DO stub
- Call `stub.updateLineup(festivalId, lineup)` via RPC

This mirrors how `ensureFestivalConfig` already calls `stub.seedLineup()`.

### 3. No protocol/protobuf changes needed

The existing `FestivalUpdate` / `SignedUpdate` / `GossipEnvelope` protobuf
messages handle this already. The Rust client's `dispatch_message` for
`FestivalUpdate` will verify the signature and apply the Yrs update — no
client-side changes needed.

## Files Modified

| File | Change |
|------|--------|
| `apps/server/src/festival-do.ts` | Add `updateLineup()` method |
| `apps/server/src/api.ts` | Forward lineup to Festival DO after MainDO upsert |

## Edge Cases

- **Festival DO not yet configured**: If `PUT /lineup` is called before any
  client has connected (i.e., before `ensureFestivalConfig` ran), the Festival
  DO won't have a Yrs doc yet. The `updateLineup()` method should handle this
  by calling `seedLineup()` if no existing doc is found, or we call
  `ensureFestivalConfig` first in the route handler.
- **No connected clients**: The broadcast loop simply iterates zero sessions —
  the update is still persisted in `gossip_log` for future `sv_exchange`/
  `catchup` requests.
