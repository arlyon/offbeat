# Known Gaps & Vulnerabilities

Identified 2026-05-21. Ordered by severity (silent data loss / broken UX first).

---

## 1. DO Hibernation Drops WebSocket Subscriptions

**File:** `apps/server/src/festival-do.ts:75-80`

**Problem:** When the Cloudflare DO hibernates, the in-memory `#sessions` Map is wiped. On wake, sessions are reconstructed with an empty topic set. Clients believe they're still subscribed but silently stop receiving broadcasts.

**Fix:** Use WebSocket attachments to persist topic subscriptions across hibernation.

```typescript
// On subscribe:
ws.serializeAttachment(JSON.stringify([...sess.topics]));

// On wake (in webSocketMessage when sess is missing):
const topics = new Set<string>(JSON.parse(ws.deserializeAttachment() || "[]"));
sess = { topics };
```

**Scope:** `festival-do.ts` only. ~10 lines changed.

---

## 2. Ghost Data on Group Leave (Yrs Anti-Pattern)

**File:** `crates/core/src/groups.rs:187`

**Problem:** `leave_group` sets `member/{user_id}` to `"{}"` instead of deleting the key. This creates a permanent entry in the CRDT doc that Yrs cannot garbage collect. Over time, as users join and leave, the doc grows unboundedly. `get_group_state` already filters these out (line 313-315), but the storage cost accumulates forever.

**Fix:** Add `remove_map_value` to `DocManager` that calls `map.remove(&mut txn, key)` to generate a proper CRDT tombstone that Yrs can GC.

```rust
// In doc_manager.rs
pub fn remove_map_value(&mut self, doc_id: &str, key: &str) -> anyhow::Result<Vec<u8>> {
    let doc = self.get_or_create(doc_id);
    let sv_before = doc.transact().state_vector();
    {
        let map = doc.get_or_insert_map("root");
        let mut txn = doc.transact_mut();
        map.remove(&mut txn, key);
    }
    let update = doc.transact().encode_state_as_update_v1(&sv_before);
    self.persist(doc_id)?;
    Ok(update)
}
```

Then change `groups.rs:187` from `set_map_value(..., "{}")` to `remove_map_value(...)`.

**Scope:** `doc_manager.rs` (new method), `groups.rs` (one-line change). Tests needed for remove + re-add cycle.

---

## 3. SQLite N+1 on Catch-Up

**File:** `crates/core/src/ws_relay.rs:437-461`

**Problem:** Catch-up loops call `dispatch_message` per entry, each triggering an independent `INSERT` with implicit auto-commit. A 500-message catch-up payload = 500 synchronous fsync operations, freezing the executor.

**Fix:** Wrap catch-up processing in an explicit transaction. `Database` holds a `Mutex<Connection>`, so add a `with_transaction` helper:

```rust
// In db/mod.rs
pub fn with_transaction<F, T>(&self, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    let conn = self.conn.lock().unwrap();
    conn.execute_batch("BEGIN")?;
    match f(&conn) {
        Ok(val) => { conn.execute_batch("COMMIT")?; Ok(val) }
        Err(e) => { conn.execute_batch("ROLLBACK")?; Err(e) }
    }
}
```

Then in `ws_relay.rs`, wrap both the chat and relay catch-up loops in `db.with_transaction(...)`.

Also applies to the relay catch-up loop at `ws_relay.rs:449-459`.

**Scope:** `db/mod.rs` (new method), `ws_relay.rs` (wrap two loops). Signature of `dispatch_message` may need a `&Connection` variant or `save_chat_message` needs a conn-accepting overload.

---

## 4. WiFi Direct Transport Missing

**Files:** `crates/core/src/transport/mod.rs` (no `wifi_direct` module), execution plan Phase 8

**Problem:** BLE and Meshtastic transport stubs exist (`transport/ble.rs`, `transport/meshtastic.rs`) but WiFi Direct is completely absent. WiFi Direct is the **primary offline high-bandwidth transport** (~250 Mbps, ~200m, standard 1500B MTU) -- it's the only offline path capable of full sync (Yrs state + chat history). Without it, the "offline-first at a festival" story collapses to BLE-range-only presence pings.

### 4a. iroh custom transport (`transport/wifi_direct.rs`)

Implement `CustomTransport`, `CustomEndpoint`, `CustomSender` traits (behind `unstable-custom-transports` feature flag). Reference: [iroh-tor](https://github.com/n0-computer/iroh-tor).

```rust
pub struct WifiDirectTransport {
    #[cfg(target_os = "android")]
    platform: AndroidWifiDirect,  // jni crate -> android.net.wifi.p2p
    #[cfg(target_os = "ios")]
    platform: IosMultipeer,       // objc2 crate -> MCSession
}

impl CustomTransport for WifiDirectTransport {
    const ID: u32 = 0x574644;  // "WFD"
    // ...
}
```

**Platform specifics:**
- **Android:** `WifiP2pManager` via JNI for discovery + connection. One device becomes group owner (AP), others connect as clients. Once connected, standard UDP sockets over local IPs.
- **iOS:** `MCSession` via objc2. Multipeer automatically selects best local transport (WiFi Direct, BLE, or infrastructure WiFi). Advertise service type `"offbeat-sync"`.

**Discovery with group context:**
- Advertise group membership in TXT records (Android) / `discoveryInfo` (iOS)
- Only connect to peers with overlapping groups
- Service instance name: first 8 chars of `node_id`

No fragmentation needed -- standard QUIC MTU works over WiFi Direct.

### 4b. Platform bridge layer

Need native bridge code for each platform:
- `android/app/src/main/kotlin/.../WifiDirectBridge.kt` -- JNI bridge to `WifiP2pManager`
- `ios/Runner/MultipeerBridge.swift` -- bridge to `MCSession`

These expose discovery/connect/send/receive to Rust via FFI.

**Scope:** New `transport/wifi_direct.rs`, platform bridge files. Medium-large effort. Phase 8.

---

## 5. P2P Chat Has No Offline Catch-Up

**Problem:** Chat messages go straight to SQLite, bypassing Yrs. The only catch-up path is via the Festival DO WebSocket (`catchup` message type). If peers are purely P2P (WiFi Direct, BLE, mesh) with no server, a rebooted phone syncs CRDT state (presence, pins, stars) but misses all chat sent while offline. This is transport-agnostic -- the protocol is simply missing.

**Protocol:** Extend the peer handshake to include chat cursors alongside Yrs state vectors:

```
Peer A connects to Peer B (any transport):

1. Exchange Yrs state vectors (existing)     -- syncs CRDTs
2. Exchange (topic, max_seq) from gossip_log  -- NEW
3. Each peer sends missing gossip_log entries -- NEW
4. Receiver runs through dispatch_message     -- existing
   (batched in a transaction per gap #3)
```

Reuses the existing `gossip_log` table (`db/schema.sql:40-46`). Wire format piggybacks on `GossipWireMessage` with a new `kind: "chat_catchup"`.

**Transport-aware sync limits:**

| Data | Internet / WiFi Direct | BLE | Meshtastic |
|------|------------------------|-----|------------|
| Festival Yrs | Full | Full | Full |
| Group Yrs | Full | Full | Full |
| Group chat | Full history | Last 50 | -- |
| Stage chat | Full history | -- | -- |

Only high-bandwidth transports should exchange chat history. BLE and Meshtastic stick to CRDT state only.

**Scope:** New `p2p_sync.rs` (or extend `gossip_manager.rs`). Phase 8.

---

## 6. LWW Array Destruction (CRDT Anti-Pattern)

**File:** `crates/core/src/groups.rs:244-247`

**Problem:** Starred sets are serialized to a JSON string and stored as a single atomic value in the Yrs map:

```rust
let stars_json = serde_json::to_string(&set_ids)?;
dm.set_map_value(&doc_id, &format!("stars/{user_id}"), &stars_json)?;
```

A `Y.Map` treats string values as atomic -- conflicts resolve via Last-Writer-Wins (LWW). If a user stars "Set A" on their phone and "Set B" on their iPad while both are offline, reconnection will arbitrarily discard one. The CRDT merge guarantee is completely bypassed.

The same pattern appears for member presence data (`member/{user_id}` stores a JSON object as a string).

**Fix:** Use nested CRDT types instead of serialized JSON:
- `stars/{user_id}` should be a `Y.Array` -- push/remove individual set IDs so concurrent adds merge correctly.
- `member/{user_id}` fields (status, stage_id, etc.) should be individual `Y.Map` entries so concurrent field updates don't clobber each other.

This requires `DocManager` methods that work with `Y.Array` and nested `Y.Map` types rather than raw string values.

**Scope:** `doc_manager.rs` (new array/nested-map methods), `groups.rs` (rewrite `update_stars`, `check_in`, `get_group_state`). Medium effort. Tests needed for concurrent offline merge scenarios.

---

## 7. Tokio Executor Starvation (std::sync::Mutex + SQLite)

**File:** `crates/core/src/db/mod.rs:12`

**Problem:** `Database` wraps `Connection` in a `std::sync::Mutex`. Every DB call acquires this mutex and performs blocking disk I/O. The gossip networking layer (`gossip_manager.rs`) runs on a Tokio async reactor. Locking a sync mutex inside `async` blocks the OS thread Tokio uses to poll network sockets.

Under festival load with hundreds of gossip messages over WiFi Direct and BLE, this causes thread starvation: dropped packets, buffer overflows, UI freezes.

**Fix:** Two options:
1. **`spawn_blocking`:** Wrap all `Database` calls in `tokio::task::spawn_blocking` so they run on Tokio's blocking thread pool instead of the async reactor.
2. **Async SQLite:** Migrate to `sqlx` or `tokio-rusqlite` for non-blocking I/O.

Option 1 is lower effort and preserves the existing `rusqlite` API. Option 2 is cleaner long-term but touches every callsite.

**Scope:** Either a wrapper layer around `Database` (option 1) or a full migration (option 2). Medium effort.

---

## 8. Unvalidated Relay Payloads (DO Denial of Service)

**File:** `apps/server/src/festival-do.ts:125-148`

**Problem:** The Festival DO blindly inserts relay payloads into `relay_log` and broadcasts to all subscribers without any validation. A malicious WebSocket client can pump arbitrary data into `festival/{id}/state`, filling Cloudflare storage and saturating mobile data for all connected clients. Clients will reject the payload (Ed25519 signature fails in `doc_manager.rs`), but the bandwidth and storage damage is already done.

**Fix:** The DO must validate relay payloads server-side before storage and broadcast:
- For `festival/{id}/state` topics: verify the Ed25519 signature before accepting. The DO already has the festival's public key.
- For group topics: at minimum, enforce a payload size cap (e.g. 64KB). Full cryptographic validation isn't possible since the DO can't read group-encrypted payloads, but size limits prevent storage abuse.
- Rate-limit per WebSocket connection (e.g. 10 relay messages/second).

**Scope:** `festival-do.ts`. Small-medium effort. Should be done before any public deployment.

---

## 9. Wall-Clock Ordering (P2P Time-Travel)

**Files:** `crates/core/src/chat.rs:185-191`, `crates/core/src/groups.rs:413-418`, `crates/core/src/db/mod.rs:169` (`ORDER BY timestamp ASC`)

**Problem:** Chat timestamps use the device's local `SystemTime::now()`. In a P2P network, wall-clock time is untrustworthy -- timezone mismatches, manual clock changes, and offline drift. A message timestamped in "2027" from a misconfigured device will permanently anchor to the bottom of chat for all peers, pushing all future 2026 messages above it.

**Fix:** Implement a Hybrid Logical Clock (HLC) for chat ordering:

```rust
struct HLC {
    wall: u64,      // max(local_time, last_received_time)
    counter: u32,   // tiebreaker for same wall time
    node_id: u64,   // final tiebreaker
}
```

HLC preserves causal ordering (if B is sent after receiving A, B sorts after A) regardless of physical clock skew. The existing `timestamp` field can remain for display purposes, but ordering must use the HLC tuple.

**Scope:** New `hlc.rs` module, update `ChatMessage` struct, update `chat.rs` send/receive, update `db/schema.sql` ordering index. Medium effort. Should be done before P2P transports ship (Phase 8).

---

## 10. Offline Passkey Lockout

**File:** `crates/core/src/auth.rs`

**Problem:** The execution plan specifies WebAuthn passkeys for auth. WebAuthn requires a server round-trip for challenge/verify. If the app is force-closed at the hotel and reopened in a dead zone, the user is locked out.

**Current state:** Auth is actually just a local Ed25519 key stored in SQLite credentials (`auth.rs:7-22`). No WebAuthn is implemented yet. This means the lockout risk doesn't exist *today*, but it will when passkeys are added.

**Fix (when adding passkeys):** Decouple local identity from server auth:
1. On first successful server auth, persist `iroh_secret` + session token in device keychain (`flutter_secure_storage`).
2. App boots offline using local identity from keychain -- no server needed.
3. WebAuthn challenge is deferred until an actual server sync is attempted.

**Scope:** Future work. Current local-key-only auth is fine for offline-first. Just don't gate app boot on WebAuthn when it's added.

---

## 11. CRDT Compaction Risk (Doc-Only, Not In Code)

**File:** `docs/sync-patterns.md:222-257`

**Problem:** The documented compaction strategy (`encode_state_as_update_v1(&StateVector::default())` into a fresh doc) destroys tombstone history. If Client A is offline during compaction, their edits to deleted nodes will resurrect data or fail to merge.

**Current state:** `compact()` and `needs_compaction()` are pseudocode in docs only -- not implemented.

**Fix (when implementing):** Use Yrs native GC instead of manual snapshot replacement. If a hard compaction is truly needed, all clients must be forced to drop local state and adopt the new snapshot (which breaks offline-first guarantees). For a festival app with bounded lifetime (~5 days), tombstone overhead is likely acceptable -- just don't compact.

**Scope:** Update `docs/sync-patterns.md` to flag the risk. No code change needed yet.

---

## 12. Meshtastic Payload Budget

**File:** `docs/sync-patterns.md`, `crates/core/src/crypto.rs`

**Problem:** AES-256-GCM adds 28 bytes (12 nonce + 16 tag) to every payload. With Meshtastic's 228-byte MTU, only 200 bytes remain. Yrs state updates will frequently exceed this, requiring fragmentation over a ~1 kbps / high-loss link.

**Fix (Phase 8):** For Meshtastic, bypass full CRDT diffing for presence. Send compact absolute payloads:

```
[1-byte msg_type][8-byte user_id_short][1-byte stage_enum][12-byte nonce][16-byte tag]
= 38 bytes total -- fits in a single frame with room to spare
```

Reserve Yrs sync and chat exchange for WiFi Direct and internet (see transport-aware sync table in gap #5).

**Scope:** Phase 8 transport layer. Design decision to document now.

---

## 13. SQLite Not in WAL Mode

**File:** `crates/core/src/db/mod.rs:22-26`, `crates/core/src/db/schema.sql`

**Problem:** The database is opened with default journal mode (DELETE). In DELETE mode, every write takes an exclusive lock on the entire database file -- readers block writers and writers block readers. With gossip messages arriving concurrently from multiple topics (chat, presence, relay) while the UI reads chat history and the sync layer reads state vectors, this becomes a serialization bottleneck. Combined with gap #7 (sync mutex on the async executor), this compounds into a full pipeline stall.

WAL (Write-Ahead Logging) mode allows concurrent readers and a single writer without blocking each other -- exactly what this workload needs.

**Fix:** Set WAL mode on connection open:

```rust
pub fn new(path: &Path) -> Result<Self> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;  // safe with WAL
    conn.pragma_update(None, "busy_timeout", "5000")?;    // 5s retry on lock
    conn.execute_batch(SCHEMA)?;
    Ok(Self { conn: Mutex::new(conn) })
}
```

`synchronous = NORMAL` is safe with WAL (durability is maintained via the WAL file) and avoids an fsync on every commit. `busy_timeout` prevents immediate `SQLITE_BUSY` errors under contention.

**Scope:** `db/mod.rs`, 3 lines added to both `new()` and `new_in_memory()`. Tiny effort. Do this immediately.

---

## 14. Thundering Herd on DO Reconnect

**Files:** `crates/core/src/ws_relay.rs`, `crates/bridge/src/api.rs:286-304`

**Problem:** When the Festival DO restarts, hibernates, or Cloudflare has a blip, all connected WebSocket clients disconnect simultaneously. There is no reconnect logic at all -- `run_receive_loop` just exits on close/error (`ws_relay.rs:234-242`). But even if reconnect is added naively, all 50,000 clients will slam the DO at the exact same instant, likely crashing it again and creating a cascading failure loop.

This is the classic thundering herd problem in distributed systems.

**Fix:** Implement reconnect with exponential backoff + jitter:

```rust
impl WsRelay {
    async fn connect_with_retry(url: &str, max_retries: u32) -> anyhow::Result<Self> {
        let mut attempt = 0;
        loop {
            match Self::connect(url).await {
                Ok(relay) => return Ok(relay),
                Err(e) if attempt < max_retries => {
                    attempt += 1;
                    // Exponential backoff: 1s, 2s, 4s, 8s... capped at 30s
                    let base = Duration::from_secs(1 << attempt.min(5));
                    // Add random jitter: 0-100% of base delay
                    let jitter = Duration::from_millis(
                        rand::random::<u64>() % base.as_millis() as u64
                    );
                    tokio::time::sleep(base + jitter).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}
```

Key properties:
- **Exponential backoff** spreads reconnects over time instead of all-at-once
- **Full jitter** (not just added jitter) decorrelates clients so they don't cluster at backoff boundaries
- **Cap at 30s** so users don't wait forever
- After reconnect, immediately send `catchup` requests to fill gaps (existing protocol)

The bridge layer (`api.rs:286-304`) should wrap `connect_relay` in a supervisor loop that calls `connect_with_retry` whenever the receive loop exits.

**Scope:** `ws_relay.rs` (new method + supervisor loop), `api.rs` (wrap spawn). Small-medium effort. Must be done before any real deployment.

---

## Priority

| # | Gap | Severity | Effort | When |
|---|-----|----------|--------|------|
| 1 | DO hibernation | **Critical** -- silent message loss | Small | Now |
| 6 | LWW array destruction | **Critical** -- silent data loss on merge | Medium | Now |
| 13 | SQLite WAL mode | **High** -- read/write contention | Tiny | Now |
| 2 | Ghost data (Yrs) | **Medium** -- unbounded growth | Small | Now |
| 3 | SQLite N+1 catch-up | **Medium** -- UI freeze | Medium | Now |
| 7 | Tokio executor starvation | **High** -- thread starvation under load | Medium | Now |
| 8 | DO relay validation | **High** -- trivial DoS | Small | Before deploy |
| 14 | Thundering herd | **High** -- cascading DO failure | Small | Before deploy |
| 9 | Wall-clock ordering | **High** -- corrupted chat order | Medium | Before Phase 8 |
| 4 | WiFi Direct transport | **High** -- no offline high-bandwidth path | Large | Phase 8 |
| 5 | P2P chat catch-up | **High** -- offline chat loss | Medium | Phase 8 |
| 10 | Passkey lockout | **Low** -- not yet implemented | Medium | When adding passkeys |
| 11 | Compaction risk | **Low** -- doc-only | Tiny | When implementing |
| 12 | Meshtastic MTU | **Low** -- Phase 8 design | Medium | Phase 8 |
