# OFFBEAT — Sync Patterns

> Strategies for efficient state synchronization, fast-forward on late join, and progressive data loading.

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

## Implementation Checklist

### Phase 4 Additions (Rust Core)
- [ ] `ChatSync` table and CRUD methods in `db/mod.rs`
- [ ] `sync_topic()` with windowing in `chat_manager.rs`
- [ ] `compact()` and `needs_compaction()` in `doc_manager.rs`
- [ ] `TopicInterest` tracking in `gossip_manager.rs`
- [ ] `SyncCoordinator` with prioritized sync in `sync.rs`

### Phase 3 Additions (Server)
- [ ] Windowed catch-up handler in `festival-do.ts`
- [ ] `serverNewestSeq` in catch-up response
- [ ] Nightly compaction job for Yrs docs

### Phase 5/7 Additions (UI)
- [ ] Progressive chat loading with scroll detection
- [ ] Loading states for chat pagination
- [ ] "Load older messages" affordance
