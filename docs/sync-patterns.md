# OFFBEAT — Sync Patterns

> Strategies for efficient state synchronization, fast-forward on late join, and progressive data loading.

## Transport Hierarchy

Offbeat schedules the same logical resources across multiple physical paths and selects a wire encoding based on each path's transport profile:

| Transport | Range | Throughput | MTU | Fragmentation | Use Case |
|-----------|-------|------------|-----|---------------|----------|
| Internet | ∞ | Varies | 1500B | No | Full connectivity |
| Wi-Fi Aware / WiFi Direct | ~200m | high | 1500B | No | No-AP local high-speed sync |
| BLE | ~10m | ~1 Mbps | 247B | Yes | Proximity, low power |
| Meshtastic card over Bluetooth | ~3km | ~1 kbps | ~228B | Avoid | Long-range constrained sync profile |

```
┌────────────────────────────────────────────────────────────┐
│ Sync scheduler chooses a profile for each available path   │
├────────────────────────────────────────────────────────────┤
│ Full:        Internet, LAN, Wi-Fi Aware, WiFi Direct       │
│ LowBandwidth: BLE                                          │
│ Constrained: Meshtastic card over Bluetooth → LoRa mesh    │
└────────────────────────────────────────────────────────────┘
```

**Sync capabilities by transport profile:**
- **Full**: Full sync — all data types, Yrs state-vector exchange, append-log catch-up, and chat history.
- **LowBandwidth**: Bounded envelopes — group/festival state and recent group chat, with strict size limits.
- **Constrained**: Compact encodings of the same logical resources — P0 festival updates, P1 group updates, P2 group chat, P3 festival chat only when idle. No bulk Yrs sync and no chat-history catch-up.

Meshtastic is therefore not a separate product/event protocol. It is the `Constrained` physical route for the shared Offbeat sync scheduler. The phone talks to a paired Meshtastic device over Bluetooth; that device carries Offbeat compact frames over LoRa `PRIVATE_APP` packets.

---

## Problem Statement

A user arriving on day 4 of a 4-day festival should not wait for days of accumulated state to sync. The app must be usable within seconds, with additional data loading progressively in the background.

Key insight: **CRDTs (Yrs) give fast-forward for free** — state vector diffs compute exactly what's needed to reach current state without replaying history. Chat is the bottleneck and requires explicit windowing.

---

## Data Volume Estimates

Realistic scenario for a 4-day festival:

| Data Type | Updates | Payload Size | Total Size |
|-----------|---------|--------------|------------|
| Festival state (Yrs) | ~30 | ~500B avg diff | ~15KB doc |
| Group state (Yrs) ×2 | ~200 each | ~200B avg diff | ~10KB per doc |
| Group chat ×2 | ~200 msgs each | ~150B/msg | ~30KB each |
| Stage chat ×6 | ~2000 msgs each | ~150B/msg | ~300KB each |

**Worst case total: ~2MB** — but a late-joiner needs only ~50KB to be fully functional.

---

## Tiered Sync Priority

### Tier 1: Immediate (Blocks App Start)

Essential state required before the app is usable:

```
Festival Yrs doc (full state) .............. ~15KB
Group Yrs docs (full state) ................ ~20KB
─────────────────────────────────────────────────
Total ...................................... ~35KB
Target ..................................... <100ms on 3G
```

### Tier 2: Eager (Background, First 30s)

Sync silently after app is interactive:

```
Group chat (last 50 msgs each) ............. ~15KB
Current stage chat (last 50 msgs) .......... ~8KB
```

### Tier 3: Lazy (On Navigate)

Fetch only when user visits:

```
Other stage chats (paginated on scroll)
Historical chat (scroll-back pagination)
```

---

## Pattern: Yrs State Vector Diff

For Yrs documents (festival state, group state), never replay individual updates. Use state vector exchange:

```rust
// Client: "Here's what I have"
let my_sv = doc.state_vector();
send(SyncRequest { sv: my_sv.encode() });

// Server: computes minimal diff
let diff = server_doc.encode_diff(&client_sv);
send(SyncResponse { diff });

// Client: applies once, now current
doc.apply_update(diff);
```

**Why this works**: Yrs docs store current state, not history. A group with 200 presence updates still results in a ~10KB doc — old presence values are overwritten, not accumulated.

### State Vector on Reconnect

```rust
impl DocManager {
    /// Sync with remote, sending only what's needed in each direction
    async fn sync(&self, doc_id: &str, remote: &Connection) -> Result<()> {
        let doc = self.get(doc_id)?;

        // Exchange state vectors
        let my_sv = doc.state_vector();
        let their_sv = remote.exchange_sv(my_sv.encode()).await?;

        // Send what they're missing
        let outgoing = doc.encode_diff(&their_sv);
        remote.send_diff(outgoing).await?;

        // Apply what we're missing (included in exchange response)
        let incoming = remote.receive_diff().await?;
        doc.apply_update(incoming)?;

        self.persist(doc_id)?;
        Ok(())
    }
}
```

---

## Pattern: Chat Windowing with Sequence Anchors

Chat messages are append-only and can grow unbounded. Use windowed sync with sequence tracking.

### Schema Extension

```sql
-- Track sync state per topic
CREATE TABLE chat_sync (
    topic       TEXT PRIMARY KEY,
    oldest_seq  INTEGER NOT NULL,  -- Oldest message we have locally
    newest_seq  INTEGER NOT NULL,  -- Newest message we have locally
    has_more    INTEGER NOT NULL DEFAULT 1  -- Server has older messages
);
```

### Sync Protocol

```typescript
// Initial sync: fetch tail only
interface CatchupRequest {
    type: "catchup";
    topic: string;
    limit: number;        // e.g., 50
    from: "latest";       // Start from newest
}

interface CatchupResponse {
    messages: ChatMessage[];
    oldest_available_seq: number;  // Server's oldest
    newest_seq: number;            // Server's newest
}

// Gap fill on reconnect
interface GapFillRequest {
    type: "catchup";
    topic: string;
    after: number;        // Fetch messages after this seq
    limit: number;
}

// Scroll-back pagination
interface ScrollBackRequest {
    type: "catchup";
    topic: string;
    before: number;       // Fetch messages before this seq
    limit: number;
}
```

### Reconnect Logic

```rust
impl ChatManager {
    async fn sync_topic(&self, topic: &str, remote: &Connection) -> Result<()> {
        let sync_state = self.db.get_chat_sync(topic)?;

        match sync_state {
            // Never synced: fetch tail
            None => {
                let resp = remote.catchup(topic, 50, CatchupFrom::Latest).await?;
                self.db.save_messages(topic, &resp.messages)?;
                self.db.save_chat_sync(topic, ChatSync {
                    oldest_seq: resp.messages.first().map(|m| m.seq).unwrap_or(0),
                    newest_seq: resp.newest_seq,
                    has_more: resp.oldest_available_seq < resp.messages.first().map(|m| m.seq).unwrap_or(0),
                })?;
            }

            // Previously synced: fill gap since last sync
            Some(state) => {
                let resp = remote.catchup_after(topic, state.newest_seq, 500).await?;
                self.db.save_messages(topic, &resp.messages)?;
                self.db.update_newest_seq(topic, resp.newest_seq)?;
            }
        }

        Ok(())
    }
}
```

---

## Pattern: Yrs Doc Compaction

Yrs documents accumulate tombstones (metadata for deleted items). For long-running festivals, periodic compaction prevents bloat.

```rust
impl DocManager {
    /// Compact doc by creating fresh snapshot without tombstones
    fn compact(&self, doc_id: &str) -> Result<()> {
        let doc = self.get(doc_id)?;

        // Encode full state (no tombstones in output)
        let snapshot = doc.encode_state_as_update_v1(&StateVector::default());

        // Create fresh doc from snapshot
        let fresh = Doc::new();
        fresh.apply_update(snapshot)?;

        // Replace and persist
        self.docs.insert(doc_id.to_string(), fresh);
        self.persist(doc_id)?;

        Ok(())
    }

    /// Check if compaction needed (>30% tombstone ratio or >100KB)
    fn needs_compaction(&self, doc_id: &str) -> bool {
        let doc = self.get(doc_id).ok()?;
        let stats = doc.stats();

        stats.tombstone_ratio() > 0.3 || stats.size_bytes() > 100_000
    }
}
```

**Schedule**: Run on Festival DO nightly. Clients receive clean state on next sync.

---

## Pattern: Progressive Chat Loading (UI)

```svelte
<script lang="ts">
    import { onMount } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';

    let { topic }: { topic: string } = $props();

    let messages = $state<ChatMessage[]>([]);
    let loading = $state(false);
    let hasMore = $state(true);
    let scrollContainer: HTMLElement;

    onMount(async () => {
        // Initial load: just the tail
        const result = await invoke<ChatWindow>('get_chat_tail', {
            topic,
            limit: 50
        });
        messages = result.messages;
        hasMore = result.hasMore;

        // Scroll to bottom
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
    });

    async function loadOlder() {
        if (loading || !hasMore) return;

        loading = true;
        const oldestSeq = messages[0]?.seq;
        const prevScrollHeight = scrollContainer.scrollHeight;

        const result = await invoke<ChatWindow>('get_chat_before', {
            topic,
            before: oldestSeq,
            limit: 50
        });

        messages = [...result.messages, ...messages];
        hasMore = result.hasMore;
        loading = false;

        // Maintain scroll position
        await tick();
        scrollContainer.scrollTop = scrollContainer.scrollHeight - prevScrollHeight;
    }

    function handleScroll(e: Event) {
        const el = e.target as HTMLElement;
        if (el.scrollTop < 100) {
            loadOlder();
        }
    }
</script>

<div
    class="chat-container"
    bind:this={scrollContainer}
    onscroll={handleScroll}
>
    {#if loading}
        <div class="loading-indicator">Loading...</div>
    {:else if hasMore}
        <button class="load-more" onclick={loadOlder}>Load older messages</button>
    {/if}

    {#each messages as msg (msg.id)}
        <ChatMessage {msg} />
    {/each}
</div>
```

---

## Pattern: Topic Interest Filtering

Avoid syncing stage chats the user hasn't visited. Track interest explicitly.

```rust
/// Topics the user has expressed interest in
struct TopicInterest {
    subscribed: HashSet<TopicId>,      // Explicitly joined
    visited: HashSet<TopicId>,         // Navigated to chat view
    auto_subscribed: HashSet<TopicId>, // From check-in
}

impl GossipManager {
    /// Only sync topics user cares about
    async fn sync_chat_topics(&self, remote: &Connection) -> Result<()> {
        let interest = self.topic_interest.lock();

        for topic in interest.all() {
            self.chat_manager.sync_topic(&topic, remote).await?;
        }

        Ok(())
    }

    /// On check-in, auto-subscribe to stage chat
    fn on_check_in(&self, stage_id: &str, festival_id: &str) {
        let topic = topics::stage_chat(festival_id, stage_id);
        self.topic_interest.lock().auto_subscribed.insert(topic);
        self.subscribe(topic);
    }
}
```

---

## Pattern: Catch-Up Prioritization

When reconnecting after extended offline period, prioritize sync order:

```rust
impl SyncCoordinator {
    async fn full_sync(&self, remote: &Connection) -> Result<()> {
        // 1. Festival state (required for app to function)
        self.doc_manager.sync("festival", remote).await?;
        self.emit_ready();  // App now usable

        // 2. Group state (required for presence)
        for group in self.db.list_groups()? {
            self.doc_manager.sync(&group.doc_id, remote).await?;
        }

        // 3. Group chat (small, high value)
        for group in self.db.list_groups()? {
            let topic = topics::group_chat(&group.key);
            self.chat_manager.sync_topic(&topic, remote).await?;
        }

        // 4. Current stage chat (if checked in)
        if let Some(stage) = self.current_stage() {
            let topic = topics::stage_chat(&self.festival_id, &stage);
            self.chat_manager.sync_topic(&topic, remote).await?;
        }

        // 5. Other subscribed chats (background, can be interrupted)
        for topic in self.topic_interest.lock().subscribed.iter() {
            tokio::task::yield_now().await;  // Allow other work
            self.chat_manager.sync_topic(topic, remote).await?;
        }

        Ok(())
    }
}
```

---

## Festival DO: Catch-Up Protocol

Update the WebSocket catch-up handler to support windowing:

```typescript
// apps/server/src/festival-do.ts

interface CatchupRequest {
    type: "catchup";
    topic: string;
    mode: "latest" | "after" | "before";
    cursor?: number;  // seq number
    limit: number;
}

async handleCatchup(ws: WebSocket, req: CatchupRequest) {
    const { topic, mode, cursor, limit } = req;

    const clampedLimit = Math.min(limit, 500);  // Server-side cap

    let messages: ChatMessage[];
    let hasMore: boolean;

    switch (mode) {
        case "latest":
            messages = await this.sql.exec(
                `SELECT * FROM chat_messages
                 WHERE topic = ?
                 ORDER BY seq DESC
                 LIMIT ?`,
                [topic, clampedLimit]
            ).reverse();
            hasMore = messages.length === clampedLimit;
            break;

        case "after":
            messages = await this.sql.exec(
                `SELECT * FROM chat_messages
                 WHERE topic = ? AND seq > ?
                 ORDER BY seq ASC
                 LIMIT ?`,
                [topic, cursor, clampedLimit]
            );
            hasMore = messages.length === clampedLimit;
            break;

        case "before":
            messages = await this.sql.exec(
                `SELECT * FROM chat_messages
                 WHERE topic = ? AND seq < ?
                 ORDER BY seq DESC
                 LIMIT ?`,
                [topic, cursor, clampedLimit]
            ).reverse();
            const oldest = await this.sql.exec(
                `SELECT MIN(seq) as min_seq FROM chat_messages WHERE topic = ?`,
                [topic]
            );
            hasMore = messages[0]?.seq > oldest[0].min_seq;
            break;
    }

    ws.send(JSON.stringify({
        type: "catchup_response",
        topic,
        messages,
        hasMore,
        serverNewestSeq: await this.getNewestSeq(topic),
    }));
}
```

---

## Recommended Defaults

| Data Type | Initial Sync | Background Sync | Pagination |
|-----------|--------------|-----------------|------------|
| Festival Yrs | Full state diff | On reconnect | N/A |
| Group Yrs | Full state diff | On reconnect | N/A |
| Group chat | Last 100 msgs | Gap fill | 50/page |
| Stage chat (current) | Last 50 msgs | Gap fill | 50/page |
| Stage chat (other) | None | On navigate | 50/page |

---

## Performance Targets

| Scenario | Target | Measurement |
|----------|--------|-------------|
| Cold start (cached) | <2s to interactive | Local SQLite load |
| Late join (day 4) | <5s to interactive | Tier 1 sync over WiFi |
| Reconnect (1hr offline) | <1s to current | State vector + gap fill |
| Chat scroll-back | <200ms per page | Paginated fetch |
| Full festival history | Background, no blocking | Progressive load |

---

## Pattern: Transport-Aware Sync

Different transports have different bandwidth constraints. The sync coordinator should adapt:

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TransportProfile {
    Full,          // WebSocket, LAN, Wi-Fi Aware, WiFi Direct
    LowBandwidth,  // BLE
    Constrained,   // Meshtastic/LoRa
}

enum SyncEncoding {
    FullEnvelope,
    BoundedEnvelope { max_bytes: usize },
    CompactFrame { max_bytes: usize },
    Suppressed,
}

impl TransportProfile {
    fn decide(self, payload: SyncPayloadKind) -> SyncEncoding {
        match (self, payload) {
            (Self::Full, _) => SyncEncoding::FullEnvelope,
            (Self::LowBandwidth, _) => SyncEncoding::BoundedEnvelope { max_bytes: 1200 },
            (Self::Constrained, FestivalUpdate | GroupUpdate | GroupChat | FestivalChat) => {
                SyncEncoding::CompactFrame { max_bytes: 200 }
            }
            (Self::Constrained, BulkCrdtSync | ChatHistory) => SyncEncoding::Suppressed,
        }
    }
}
```

**Encoding by transport profile:**

| Data | Full | LowBandwidth/BLE | Constrained/Meshtastic |
|------|------|------------------|------------------------|
| Festival update | Full envelope / catch-up | Bounded envelope | Compact signed update |
| Group update | Encrypted Yrs diff | Bounded encrypted diff | Compact encrypted op |
| Group chat | Full append log | Recent bounded batch | Short encrypted message |
| Festival chat | Full append log | Usually suppressed | Idle-only compact message |
| Bulk Yrs/chat history | ✓ | Bounded | Suppressed |

This ensures:
- Resources keep the same logical semantics on every path
- The wire encoding adapts to the available path
- Meshtastic is a constrained transport profile, not a separate event protocol
- Mesh links aren't saturated with bulk catch-up/history

---

---

## Lineup Updates from Clashfinder

When lineup data changes on Clashfinder (new artists, time changes, cancellations), the server needs to detect changes and push updates to connected clients.

### Architecture Overview

```
┌─────────────────┐     poll      ┌──────────────────┐
│  Clashfinder    │◄─────────────│    Main DO       │
│  API            │              │  (festival reg)  │
└─────────────────┘              └────────┬─────────┘
                                          │
                                          │ POST /broadcast-lineup
                                          ▼
                                 ┌──────────────────┐
                                 │   Festival DO    │
                                 │   (per-festival) │
                                 └────────┬─────────┘
                                          │
                          ┌───────────────┼───────────────┐
                          │               │               │
                          ▼               ▼               ▼
                     ┌────────┐      ┌────────┐      ┌────────┐
                     │Client A│      │Client B│      │Client C│
                     └────────┘      └────────┘      └────────┘
```

### Update Detection Flow

1. **Trigger**: Cron job or manual `POST /festivals/:id/refresh`
2. **Fetch**: Call Clashfinder API with auth credentials
3. **Parse**: Convert API response to `Lineup` using `parseClashfinderApi()`
4. **Diff**: Compare serialized JSON with last stored lineup
5. **Store**: If changed, insert new lineup into `festival_history`
6. **Notify**: POST to Festival DO's `/broadcast-lineup` endpoint

### Main DO: Refresh Endpoint

```typescript
// POST /festivals/:id/refresh
// POST /festivals/refresh-all

async #refreshFestival(source: ClashfinderSource, auth: ClashfinderAuth) {
    // 1. Fetch from Clashfinder API
    const response = await fetchClashfinder(source.clashfinderId, auth);
    const newLineup = parseClashfinderApi(source.festivalId, response, {
        name: source.name,
        location: source.location,
    });

    // 2. Compare with current
    const currentLineup = this.#getLineup(source.festivalId);
    if (JSON.stringify(currentLineup) === JSON.stringify(newLineup)) {
        return { updated: false };
    }

    // 3. Store new version in history
    this.sql.exec(
        `INSERT INTO festival_history (festival_id, data) VALUES (?, ?)`,
        source.festivalId,
        JSON.stringify(newLineup),
    );

    // 4. Sync stages table
    this.#syncStages(source.festivalId, newLineup.stages);

    // 5. Notify Festival DO
    await this.#notifyFestivalDO(source.festivalId, newLineup);

    return { updated: true };
}

async #notifyFestivalDO(festivalId: string, lineup: Lineup) {
    const doId = this.env.FESTIVAL_DO.idFromName(festivalId);
    const stub = this.env.FESTIVAL_DO.get(doId);

    await stub.fetch(new Request("http://internal/broadcast-lineup", {
        method: "POST",
        body: JSON.stringify(lineup),
    }));
}
```

### Festival DO: Broadcast Handler

```typescript
// Handle internal lineup broadcast request
if (method === "POST" && path === "/broadcast-lineup") {
    const lineup = await request.json() as Lineup;

    // Store in relay log for catch-up
    const result = this.sql.exec(
        "INSERT INTO relay_log (topic, data) VALUES (?, ?) RETURNING seq",
        "lineup",
        JSON.stringify(lineup),
    ).one() as { seq: number };

    // Broadcast to all clients subscribed to "lineup" topic
    const broadcast = JSON.stringify({
        type: "relay",
        topic: "lineup",
        seq: result.seq,
        data: lineup,
    });

    for (const [ws, session] of this.#sessions) {
        if (session.topics.has("lineup")) {
            ws.send(broadcast);
        }
    }

    return new Response("OK");
}
```

### Client: Subscribe to Lineup Updates

```typescript
// On connect, subscribe to lineup topic
ws.send(JSON.stringify({
    type: "subscribe",
    topics: ["lineup", "chat:global"],
}));

// Handle incoming lineup updates
ws.onmessage = (event) => {
    const msg = JSON.parse(event.data);

    if (msg.type === "relay" && msg.topic === "lineup") {
        // Update local lineup state
        lineupStore.set(msg.data as Lineup);

        // Optionally show UI notification
        toast("Lineup updated!");
    }
};

// On reconnect, catch up from last known sequence
ws.send(JSON.stringify({
    type: "catchup",
    topic: "lineup",
    sinceSeq: lastKnownSeq,
}));
```

### Scheduling Updates

Use Cloudflare Cron Triggers to poll periodically:

```toml
# wrangler.toml
[triggers]
crons = ["0 */6 * * *"]  # Every 6 hours
```

```typescript
// In worker
export default {
    async scheduled(event: ScheduledEvent, env: Env) {
        const mainDO = env.MAIN_DO.get(env.MAIN_DO.idFromName("main"));
        await mainDO.fetch(new Request("http://internal/festivals/refresh-all", {
            method: "POST",
        }));
    },
};
```

### Environment Setup

```bash
# Set Clashfinder credentials
wrangler secret put CLASHFINDER_USERNAME
wrangler secret put CLASHFINDER_PRIVATE_KEY

# Local development
cp apps/server/.dev.vars.example apps/server/.dev.vars
# Edit with your credentials
```

### Adding New Festivals

Edit `apps/server/src/sources.ts`:

```typescript
export const FESTIVAL_SOURCES: ClashfinderSource[] = [
    {
        festivalId: "fieldday2026",
        clashfinderId: "fieldday2026",  // from clashfinder.com/s/{this}/
        name: "Field Day 2026",
        location: "Victoria Park, London",
        city: "London",
        country: "GB",
        genres: ["Electronic", "Indie"],
    },
    // Add more festivals here
];
```

---

## Implementation Checklist

### Phase 4 Additions (Rust Core)
- [ ] `ChatSync` table and CRUD methods in `db/mod.rs`
- [ ] `sync_topic()` with windowing in `chat_manager.rs`
- [ ] `compact()` and `needs_compaction()` in `doc_manager.rs`
- [ ] `TopicInterest` tracking in `gossip_manager.rs`
- [ ] `SyncCoordinator` with prioritized sync in `sync.rs`
- [ ] `TransportTier` enum and `tier_for_peer()` in `sync.rs`
- [ ] Transport-aware sync limits in `sync_with_peer()`

### Phase 3 Additions (Server)
- [ ] Windowed catch-up handler in `festival-do.ts`
- [ ] `serverNewestSeq` in catch-up response
- [ ] Nightly compaction job for Yrs docs
- [ ] `POST /festivals/:id/refresh` endpoint in `main-do.ts`
- [ ] `POST /festivals/refresh-all` endpoint in `main-do.ts`
- [ ] `/broadcast-lineup` internal handler in `festival-do.ts`
- [ ] Cron trigger for periodic Clashfinder polling
- [ ] Use `FESTIVAL_SOURCES` whitelist + Clashfinder API instead of fixtures

### Phase 5/7 Additions (UI)
- [ ] Progressive chat loading with scroll detection
- [ ] Loading states for chat pagination
- [ ] "Load older messages" affordance

### Phase 8 Additions (P2P Transports)
- [ ] `WifiDirectTransport` with platform bridges (Android/iOS)
- [ ] `BleTransport` with fragmentation layer
- [ ] `MeshtasticTransport` with fragmentation + protobuf
- [ ] Fragmentation module shared by BLE and Meshtastic
- [ ] Multi-transport status UI with expanded detail view
- [ ] Android `WifiDirectBridge.kt` JNI bridge
- [ ] iOS `MultipeerBridge.swift` bridge
