# OFFBEAT — Execution Plan

8 phases, executed sequentially via isolated sub-agents. Each phase runs in a git worktree, creates a stacked branch, and must pass `pnpm check` before completion.

---

## Phase 1: Scaffold Monorepo

**Goal:** Monorepo with all workspaces, build tooling, and passing `pnpm check`.

**Dependencies:** None

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are setting up a monorepo for "OFFBEAT", a festival timeline tracker built with Tauri v2 + SvelteKit + Cloudflare Workers.

## Create the following structure

### Root
- `pnpm-workspace.yaml`: packages: ["apps/*", "packages/*"]
- `turbo.json`: pipeline with "check" task that runs in all packages + root
- Root `package.json`: name "offbeat", private, packageManager "pnpm@10.33.0", scripts: { "check": "turbo check", "check:rust": "cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace" }. The turbo check pipeline should include root tasks so check:rust runs too.
- `biome.json`: indent with tabs, line width 100, organize imports enabled, linter recommended rules
- `Cargo.toml` (workspace): members = ["apps/mobile/src-tauri"], resolver = "2", edition = "2024"
- `.cargo/config.toml`: alias clippy-all = "clippy --workspace --all-targets"
- `.gitignore`: node_modules, target, .svelte-kit, .wrangler, dist, *.sqlite

### apps/mobile (SvelteKit)
- Init SvelteKit: TypeScript, no demo app, adapter-static with fallback "index.html" (SPA mode)
- `svelte.config.js` using adapter-static
- `vite.config.ts` with sveltekit plugin
- `tsconfig.json` extending SvelteKit defaults
- `src/app.html` — basic HTML shell with viewport meta
- `src/routes/+layout.svelte` — empty layout
- `src/routes/+page.svelte` — placeholder text "OFFBEAT"
- `src/lib/styles/tokens.css` — empty file (Phase 5)
- package.json: name "@offbeat/mobile", scripts: { "check": "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json && biome check src/", "dev": "vite dev", "build": "vite build" }

### apps/mobile/src-tauri (Tauri v2)
- `Cargo.toml`: tauri v2, serde, serde_json, tokio (full). Edition 2024.
- `tauri.conf.json`: app name "offbeat", identifier "com.offbeat.app", devUrl "http://localhost:5173", frontendDist "../build"
- `src/main.rs`: standard Tauri v2 entry (`tauri::Builder::default().run()`)
- `src/lib.rs`: empty, just module declarations commented out for future modules
- `build.rs`: `tauri_build::build()`

### apps/server (Cloudflare Workers)
- `package.json`: name "@offbeat/server", scripts: { "check": "tsc --noEmit && biome check src/", "dev": "wrangler dev" }
- `tsconfig.json`: strict, ESNext target/module, types: ["@cloudflare/workers-types"]
- `wrangler.toml`: name "offbeat-server", compatibility_date "2025-05-01", main "src/index.ts"
- `src/index.ts`: minimal Hono app returning "OFFBEAT API" on GET /
- Install: @cloudflare/workers-types, wrangler (devDeps), hono

### packages/protocol
- `package.json`: name "@offbeat/protocol", scripts: { "check": "tsc --noEmit && biome check src/" }
- `tsconfig.json`: strict, declaration, composite
- `src/index.ts`: export placeholder `export interface Festival { id: string; name: string; }`

## After creating everything
- Run `pnpm install`
- Run `pnpm check` and fix ALL errors until clean
- Initialize git: `git init && git add -A && git commit -m "initial commit"`
- Initialize git-spice: `git spice init`
- Create branch: `git spice branch create phase-1-scaffold`
- Commit: `git spice commit create -m "scaffold monorepo with turborepo, sveltekit, tauri v2, cloudflare workers"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, pnpm check status, decisions made.
```

---

## Phase 2: Protocol & Data Model

**Goal:** All shared types, parsers, Rust types, SQLite schemas, and fixture data.

**Dependencies:** Phase 1

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing the data model for OFFBEAT. Phase 1 (scaffold) is complete. Read existing code first.

## packages/protocol/src/types.ts

Export these TypeScript interfaces:

- Festival: id, name, year, location, city, country, startDate, endDate, stages: Stage[], genres: string[], status: "upcoming"|"live"|"past", publicKey: string, updatedAt
- Stage: id, name, short, color, order
- Day: id, label, num, month
- Set: id, day, stage, artist, startMin (minutes from midnight), durationMin, genre, cancelled
- Lineup: festival (Pick<Festival, "id"|"name"|"location">), stages, days, sets
- MemberLocation: userId, displayName, stageId (nullable), customLocation (nullable), status: "active"|"idle", updatedAt
- GroupPin: id, label, location, pinnedBy, createdAt
- ChatMessage: id, userId, text, topic, timestamp
- TransportStatus: mode: "full"|"local"|"mesh"|"offline", wsConnected, blePeers, meshConnected
- SignedUpdate: update (Uint8Array as base64 in JSON), author, signature

## packages/protocol/src/clashfinder.ts

Parser for Clashfinder JSON format:

```typescript
interface ClashfinderEvent {
  artist: string;
  stage: string;
  day: string;       // "friday", "saturday"
  start: string;     // "18:00"
  end: string;       // "19:30"
}
export function parseClashfinder(festivalId: string, events: ClashfinderEvent[]): Lineup;
```

Generate stable set IDs from content. Derive Stage and Day objects from the events. Parse time strings to minutes-from-midnight.

## packages/protocol/src/clashfinder.test.ts

Test with fixture data. Verify correct number of stages, days, sets. Verify time parsing. Use vitest.

## packages/protocol/fixtures/fieldday26.json

~30 ClashfinderEvent entries across 6 stages (STAGE 1, STAGE 2, RED ROOM, STAGE 4, BARN, OUTPOST), 2 days (friday, saturday). Use artists from the design prototypes: Four Tet, Bicep, Floating Points, Caribou, Aphex Twin, Jamie xx, Overmono, Romy, Bonobo, Sherelle, Helena Hauff, ANNA, SPFDJ, Peggy Gou, Burial, Skee Mask, etc. Realistic times (18:00-02:00).

## apps/mobile/src-tauri/src/types.rs

Mirror protocol types in Rust with serde derives. Snake_case fields, serde(rename_all = "camelCase") for JSON interop.

## apps/mobile/src-tauri/src/ipc_types.rs

Rust structs for Tauri IPC:
- Commands: GetFestivals, GetLineup(festival_id), StarSet(festival_id, set_id), CheckIn(group_id, stage_id_or_custom), SendChat(topic, text), CreateGroup(festival_id, name), JoinGroup(invite_payload), LeaveGroup(group_id), PinLocation(group_id, label, location), GetGroups(festival_id), GetTransportStatus, SubscribeChat(topic), UnsubscribeChat(topic)
- Events: FestivalsUpdated, LineupUpdated, GroupStateUpdated, ChatMessageReceived, TransportChanged

## apps/mobile/src-tauri/src/db/schema.sql

Client SQLite schema (from the PRD — docs, groups, chat_messages, credentials, starred_sets, gossip_log tables).

## apps/mobile/src-tauri/src/db/mod.rs

Database struct using rusqlite with "bundled" feature:
- new(path) → runs schema.sql
- save_doc / load_doc / list_docs
- save_group / load_groups / delete_group
- toggle_star / get_stars
- save_chat_message / get_chat_messages(topic, limit, offset)
- save_gossip / get_gossip(topic, since_seq)

Write unit tests for each method using an in-memory SQLite DB.

## apps/server/src/schema.sql

Server SQLite schema (from PRD — festivals, festival_history, credentials tables).

## After implementing
- Add vitest to protocol package, wire "test" script into turbo check (or run separately)
- Run `pnpm check` — fix all errors
- Run `cargo test` — fix all errors
- Stack branch: `git spice branch create phase-2-protocol` (onto phase-1-scaffold)
- Commit: `git spice commit create -m "define protocol types, clashfinder parser, rust types, sqlite schemas"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, test results, decisions made.
```

---

## Phase 3: Server — Main DO + Festival DO

**Goal:** Cloudflare Workers backend with REST API, passkey auth, festival DO as gossip peer + blind mailbox.

**Dependencies:** Phase 2

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing the Cloudflare Workers server for OFFBEAT. Phases 1-2 are complete. Read existing code first — especially packages/protocol and apps/server/src/schema.sql.

## apps/server/src/api.ts

Hono router:
- GET /festivals → list all from Main DO
- GET /festivals/:id → single festival with lineup
- GET /festivals/:id/lineup → raw Clashfinder JSON
- POST /auth/register/begin → WebAuthn registration options
- POST /auth/register/complete → store credential, return user_id
- POST /auth/authenticate/begin → WebAuthn assertion options
- POST /auth/authenticate/complete → verify, return JWT (24h expiry)
- WebSocket upgrade: /festivals/:id/ws → route to Festival DO

## apps/server/src/main-do.ts

Main Durable Object with SQLite storage (this.ctx.storage.sql):
- On init: run schema.sql, seed with Field Day 2026 fixture data (parse from @offbeat/protocol fixtures)
- listFestivals(): query all festivals
- getFestival(id): query single with lineup
- HTTP handler: forward Hono-routed requests
- Credential storage for WebAuthn

## apps/server/src/festival-do.ts

Festival Durable Object:
- On first connection: validate active window (start - 1 day to end + 1 day). Reject if outside.
- Initialize Yrs doc from lineup JSON (use yjs package). Structure: Y.Map with meta, stages, sets, announcements.
- Generate signing keypair (store in DO storage). Publish public key to main DO.
- WebSocket handler (using hibernation API):
  - On connect: client joins gossip. Send current Yrs state as signed initial payload.
  - On message from client: 
    - Chat messages: store in SQLite history, broadcast to other connections on same topic
    - Group relay messages: store as encrypted blobs, broadcast to connections subscribed to that group topic
  - Topic subscription: client sends { type: "subscribe", topics: string[] }
  - Catch-up: client sends { type: "catchup", topic, since_seq } → DO replays stored messages
- On window close: serialize all state, POST to main DO /history, clear local state

NOTE: The Festival DO acts as a WebSocket-based gossip relay for now. It doesn't run a full iroh node (Workers can't bind UDP). Clients connect to it via WebSocket for the cloud path, and use iroh for P2P paths. The DO speaks a simple JSON+binary protocol over WebSocket, not iroh's native protocol. This is the pragmatic choice — the DO is just one relay in the network.

## apps/server/src/auth.ts

WebAuthn utilities using @simplewebauthn/server:
- generateRegistrationOptions, verifyRegistration
- generateAuthenticationOptions, verifyAuthentication
- JWT signing/verification (use jose package)

## apps/server/src/signing.ts

Ed25519 signing for lineup updates:
- generateKeypair() → { publicKey, secretKey }
- sign(secretKey, data) → signature
- verify(publicKey, data, signature) → boolean

Use @noble/ed25519 or tweetnacl.

## wrangler.toml updates

```toml
[[durable_objects.bindings]]
name = "MAIN_DO"
class_name = "MainDO"

[[durable_objects.bindings]]
name = "FESTIVAL_DO"
class_name = "FestivalDO"

[[migrations]]
tag = "v1"
new_sqlite_classes = ["MainDO", "FestivalDO"]
```

## Dependencies to add
yjs, hono, @simplewebauthn/server, jose, @noble/ed25519, @offbeat/protocol

## After implementing
- Run `pnpm check` — fix all errors
- Stack branch: `git spice branch create phase-3-server` (onto phase-2-protocol)
- Commit: `git spice commit create -m "implement main DO, festival DO with gossip relay and signed lineup"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, architecture decisions, limitations.
```

---

## Phase 4: Rust Core — iroh + Yrs + Gossip

**Goal:** iroh endpoint, Yrs doc manager, gossip integration, encryption, signing verification, and Tauri IPC bridge.

**Dependencies:** Phase 3

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing the Rust core for OFFBEAT's Tauri v2 client. Phases 1-3 are complete. Read existing code — especially src/types.rs, src/ipc_types.rs, src/db/mod.rs.

## Cargo.toml dependencies

Add to apps/mobile/src-tauri/Cargo.toml:
- iroh = { version = "1", features = ["unstable-custom-transports"] }
- iroh-gossip = "0.99"
- yrs = "0.22" (or latest)
- tokio = { version = "1", features = ["full"] }
- serde = { version = "1", features = ["derive"] }
- serde_json = "1"
- rusqlite = { version = "0.36", features = ["bundled"] }
- aes-gcm = "0.10"
- ed25519-dalek = { version = "2", features = ["rand_core"] }
- sha2 = "0.10"
- blake3 = "1"
- rand = "0.9"
- base64 = "0.22"
- uuid = { version = "1", features = ["v4"] }
- tauri = { version = "2", features = [] }
- anyhow = "1"
- tracing = "0.1"
- tracing-subscriber = "0.3"

Check that these versions exist on crates.io and adjust if needed. Use `cargo add` where possible.

## src/doc_manager.rs

DocManager manages Yrs documents:

```rust
pub struct DocManager {
    docs: HashMap<String, yrs::Doc>,
    db: Arc<Database>,
}
```

Methods:
- new(db) → loads all persisted docs from SQLite
- get_or_create(doc_id, doc_type) → &Doc
- apply_update(doc_id, update: &[u8]) → persist after apply
- get_state_vector(doc_id) → Vec<u8>
- encode_diff(doc_id, remote_sv: &[u8]) → Vec<u8>
- persist(doc_id) → save to SQLite

Festival doc helpers:
- apply_signed_update(doc_id, signed_update: SignedUpdate, festival_public_key: &[u8]) → verify ed25519 signature, then apply. Reject if invalid.
- read_lineup(doc_id) → Lineup (deserialize Yrs doc to typed struct)

Group doc helpers:
- apply_encrypted_update(doc_id, encrypted: &[u8], group_key: &[u8; 32]) → decrypt AES-256-GCM, then apply
- encrypt_update(update: &[u8], group_key: &[u8; 32]) → Vec<u8>
- check_in(doc_id, user_id, stage_id_or_custom) → Yrs transaction on group doc
- update_stars(doc_id, user_id, set_ids: Vec<String>) → Yrs transaction
- add_pin(doc_id, pin) → Yrs transaction
- add_member(doc_id, user_id, display_name) → Yrs transaction
- remove_member(doc_id, user_id) → Yrs transaction
- read_group_state(doc_id) → GroupState struct

## src/crypto.rs

- generate_group_key() → [u8; 32]
- encrypt(key, plaintext) → Vec<u8> (AES-256-GCM, random nonce prepended)
- decrypt(key, ciphertext) → Result<Vec<u8>>
- group_id_from_key(key) → String (blake3 hash, hex first 16 bytes)
- verify_signature(public_key, data, signature) → bool (ed25519)

## src/topics.rs

Deterministic topic ID derivation:

```rust
pub fn festival_topic(festival_id: &str, channel: &str) -> iroh_gossip::net::TopicId {
    let hash = blake3::hash(format!("offbeat/{festival_id}/{channel}").as_bytes());
    TopicId::from_bytes(*hash.as_bytes())  // TopicId is [u8; 32]
}

pub fn group_topic(group_key: &[u8; 32], channel: &str) -> iroh_gossip::net::TopicId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(group_key);
    hasher.update(channel.as_bytes());
    TopicId::from_bytes(*hasher.finalize().as_bytes())
}
```

Channels: "state" for festival/group Yrs updates, "chat/{stage_id}" for stage chat, "chat" for group chat, "campsite", "general".

## src/gossip_manager.rs

Manages iroh-gossip subscriptions:

```rust
pub struct GossipManager {
    gossip: iroh_gossip::net::Gossip,
    subscriptions: HashMap<TopicId, GossipSubscription>,
    doc_manager: Arc<Mutex<DocManager>>,
    db: Arc<Database>,
}
```

- subscribe(topic_id, bootstrap_peers) → join gossip swarm
- unsubscribe(topic_id)
- publish(topic_id, data: Vec<u8>) → broadcast to swarm
- Message dispatch loop: incoming gossip message → match topic:
  - Festival state topic → verify signature → apply to DocManager
  - Group state topic → decrypt → apply to DocManager
  - Chat topic → store in SQLite chat_messages table
  - Group chat topic → decrypt → store in SQLite

## src/ipc.rs

Tauri IPC bridge:

Commands:
- get_festivals() → read from local DB
- get_lineup(festival_id) → DocManager.read_lineup()
- star_set(festival_id, set_id) → toggle in DB + update group Yrs docs
- check_in(group_id, stage_id_or_custom) → DocManager.check_in() + gossip publish
- send_chat(topic, text) → create ChatMessage, gossip publish (encrypt if group)
- create_group(festival_id, name) → generate key, create doc, add self as member, return QR payload
- join_group(invite_payload) → parse, store key, create doc, add self as member
- leave_group(group_id) → remove self from doc, delete key from DB
- pin_location(group_id, label, location) → DocManager.add_pin() + gossip publish
- get_groups(festival_id) → from DB
- get_transport_status() → from iroh endpoint
- subscribe_chat(topic) → GossipManager.subscribe()
- unsubscribe_chat(topic) → GossipManager.unsubscribe()

Events (emit to webview via tauri::Emitter):
- festivals_updated, lineup_updated, group_state_updated, chat_message, transport_changed

Set up Yrs doc observation callbacks that emit IPC events when docs change.

## src/lib.rs

Wire everything:
1. Open/create SQLite DB at tauri app data dir
2. Create DocManager with DB
3. Create iroh::Endpoint (IP + relay, presets::N0)
4. Create iroh_gossip::net::Gossip with endpoint
5. Create GossipManager with gossip + doc_manager + db
6. Register all Tauri IPC commands
7. Spawn gossip message dispatch loop on tokio runtime

## Tests

Write cargo tests for:
- DocManager: create, mutate, persist, reload, verify state matches
- DocManager: signed update verification (valid sig → applied, invalid → rejected)
- DocManager: encryption round-trip (encrypt update → decrypt → apply → state matches)
- crypto: encrypt/decrypt round-trip, group_id derivation deterministic
- topics: deterministic topic derivation, same inputs → same TopicId

## After implementing
- cargo test — all pass
- pnpm check — all pass
- Stack branch: `git spice branch create phase-4-rust-core` (onto phase-3-server)
- Commit: `git spice commit create -m "implement iroh endpoint, yrs doc manager, gossip integration, tauri ipc"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, test results, iroh/yrs integration details, decisions made.
```

---

## Phase 5: UI — App Shell + Festival Views

**Goal:** Full Svelte UI with design system, app shell, festival list, and festival detail views.

**Dependencies:** Phase 4

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing the Svelte UI for OFFBEAT. Phases 1-4 are complete. The Rust core handles all state. Your job is a pixel-perfect rendering layer.

Read the design prototype files to understand the visual language. They are extracted at:
- App shell variants: look in the docs/ directory or the extracted design bundles
- The design system tokens are defined below

## Design System

Create `src/lib/styles/tokens.css` with ALL these tokens:

```css
:root {
  --void: #000000;
  --bg: #0B0B0C;
  --surface-1: #131315;
  --surface-2: #1B1B1E;
  --surface-3: #232327;
  --hairline: #2A2A2E;
  --dotted: #3A3A40;
  --fg: #F2F0EA;
  --fg-2: #B8B6B0;
  --fg-3: #7A7873;
  --fg-4: #4A4845;
  --accent: #FF2D8F;
  --accent-ink: #0B0B0C;
  --accent-dim: #B81E68;
  --accent-wash: #2A0F1E;
  --co-accent: #3DDBD9;
  --warn: #FFB347;
  --ok: #9BE15D;
  --err: #FF4D4D;
  --stage-1: #FF2D8F;
  --stage-2: #3DDBD9;
  --stage-3: #FFB347;
  --stage-4: #9BE15D;
  --stage-5: #C77DFF;
  --stage-6: #FF8C42;
  --font-sans: Helvetica, "Helvetica Neue", Arial, sans-serif;
  --font-mono: "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  --t-display: clamp(40px, 11vw, 72px);
  --t-h1: clamp(28px, 7vw, 40px);
  --t-h2: 20px;
  --t-h3: 16px;
  --t-body: 15px;
  --t-small: 13px;
  --t-meta: 11px;
  --dur-1: 80ms;
  --dur-2: 140ms;
  --dur-3: 220ms;
  --dur-4: 380ms;
  --ease: cubic-bezier(0.2, 0.7, 0.2, 1);
  --tap: 44px;
  --nav-h: 52px;
  --tab-h: 56px;
}
```

Key rules:
- ZERO border-radius. Square edges everywhere. Exception: live-dot (pulsing circle).
- Borders: 1.5px dotted var(--dotted)
- Helvetica for headings/body, JetBrains Mono for data/labels/meta
- Meta labels: uppercase, letter-spacing 0.08em, mono font, fg-3 color
- Import JetBrains Mono from Google Fonts in app.html

## Components to build (use Svelte 5 runes: $state, $derived, $effect)

### App Shell (Variant A — "Index")
- **+layout.svelte**: full-height flex column, imports tokens.css + global.css, provides page slot
- **StatusBar.svelte**: 28px, mono font, shows time + "OFFBEAT" + battery (static values fine)
- **Mark.svelte**: 3-bar equalizer (9px, 14px, 6px heights, third bar accent color)
- **TopNav.svelte**: 52px, dotted bottom border. Props: showBack, festival name. Mark + "OFFBEAT//" wordmark left, action buttons right.
- **TabBar.svelte**: 56px, dotted top border, 4-column grid (FESTIVALS, SCHEDULE, NOW with live dot, YOU). Active: accent color + 1.5px accent line on top.
- **ConnectionStatus.svelte**: small transport mode indicator (●/◐/◉/○ + label)

### Festival List (+page.svelte)
- "Festivals." title (34px bold)
- Subtitle: "X ACTIVE · Y SAVED · SYNC HH:MM" (mono, 11px, fg-3)
- **SearchBar.svelte**: dotted border, search icon, placeholder, ⌘K badge, clear button
- "// SAVED" eyebrow with star count pill (accent bg)
- "// DISCOVER" eyebrow with festival count
- **FestivalRow.svelte**: grid [68px art | body]. Art = **FestArt.svelte** (gradient bg by hue index + grain overlay). Body = name + live badge + dates + city + stages + genre.
- Empty state: "NO RESULTS // {query}"

### Festival Detail (routes/festival/[id]/+page.svelte)
- TopNav with back chevron + "OFFBEAT // {name}"
- Day pills: selectable, dotted border inactive, solid + filled active
- View mode selector (Gantt / Day / Stage / Filter)

### GanttView.svelte (V1 — the signature view)
Match the V1GanttScroll prototype exactly:
- Time X axis (18:00–02:00), 6 stage rows on Y
- CONTENT_W = 480min × 3px/min = 1440px
- Stage labels (46px) sticky left
- SCROLL INTERACTION: user scrolls vertically → gantt translates horizontally. Use a scroll sentinel div (~1500px height). Map scrollTop / maxScroll to progress (0-1). Translate content by progress × maxTranslateX.
- Time axis: hour ticks + half-hour marks. "Centered time" badge pinned top-right.
- Set blocks: absolute positioned, colored left border per stage, artist name + time. Starred: accent-wash bg. Live: accent border glow.
- NOW line: 2px accent vertical line with 8px dot at top
- Bottom HUD: scrubber bar + "scroll ↓" hint with bobbing animation

### DayTabs.svelte (V2)
- Ticket-stub day picker: grid buttons with month/dow/date-number/set-count
- Active day: accent indicators
- Hour-grouped set list with sticky hour headers
- **SetRow.svelte**: grid [56px time | 4px color bar | name + sub | star]

### StageTabs.svelte (V3)
- Horizontal scrolling stage tabs with color swatch + name + count + live flag
- Day pill row above
- Stage hero card: accent stripe left, "// STAGE PROFILE", big name, meta
- Now-on-stage callout (if live)
- Lineup with BigCard components

### FilterPanel.svelte (V4)
- Bottom sheet (absolute positioned, slides up)
- Grip handle, "Filters" title, "CLEAR ALL" button
- Stage grid (2-column, colored swatches, checkbox indicator)
- Time range (visual track + labels — doesn't need to be draggable, static display is fine for V1)
- Genre chips (wrap row, dotted inactive, accent active)
- Smart toggles: starred only, hide clashes (switch with sliding indicator)
- Footer: RESET ghost + "SHOW X SETS →" primary button
- Active filter summary bar behind sheet

### Shared
- **FestArt.svelte**: gradient tile (5 hue palettes from design), SVG grain overlay
- **Chip.svelte**: mono uppercase, dotted border, accent active
- **LiveDot.svelte**: 7px pulsing magenta circle, CSS box-shadow animation
- **Icon.svelte**: wrapper for Lucide icons (install lucide-svelte)

## IPC wiring

Create `src/lib/ipc.ts`:
```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
// Typed wrappers for all commands and event listeners
// Graceful fallback: if invoke fails (no Tauri runtime, e.g. dev mode), return mock data
```

Wire:
- Festival list: invoke("get_festivals") on mount, listen("festivals_updated")
- Lineup: invoke("get_lineup", { festivalId }) on festival page mount, listen("lineup_updated")
- Stars: invoke("star_set", { festivalId, setId }) on star toggle
- Transport: listen("transport_changed") for ConnectionStatus

## Page transitions
- Tab switches: Svelte crossfade or fly transitions
- Festival list → detail: slide left
- Detail → list: slide right
- Filter sheet: fly from bottom

## Important
- Mobile-first. 390px width. All tap targets ≥ 44px.
- Svelte 5 runes ($state, $derived, $effect). NOT legacy reactive syntax.
- Run `pnpm check` — fix all errors
- Stack branch: `git spice branch create phase-5-ui` (onto phase-4-rust-core)
- Commit: `git spice commit create -m "implement UI: app shell, festival list, gantt, day tabs, stage tabs, filters"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, component inventory, design decisions, known issues.
```

---

## Phase 6: Auth + Friend Groups

**Goal:** Passkey auth, group lifecycle, QR join, presence, shared stars, pins.

**Dependencies:** Phase 5

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing auth and friend groups for OFFBEAT. Phases 1-5 complete. Read existing code.

## Rust side

### src/auth.rs
- register_passkey(server_url): call server /auth/register/begin, bridge to webview for WebAuthn ceremony, call /auth/register/complete, store user_id + credential
- authenticate(server_url): call server /auth/authenticate/begin, bridge to webview, call /auth/authenticate/complete, store JWT
- iroh identity: generate ed25519 SecretKey on first launch, store in credentials table. User ID = hex(NodeId).

### src/groups.rs
- create_group(festival_id, name, user_id, display_name) → generate AES key, derive group_id (blake3), create Yrs doc, add self to members, save group to DB, build QR payload: "offbeat://group/{group_id}/{base64url(key)}/{node_id}", subscribe to gossip topics. Return QR payload.
- join_group(invite_payload, user_id, display_name) → parse payload, store key + group in DB, create Yrs doc, add self to members, subscribe to gossip topics.
- leave_group(group_id, user_id) → remove self from members in Yrs doc, publish update via gossip, delete group key from DB, unsubscribe gossip topics.
- Wire all into ipc.rs commands.

### Update gossip_manager.rs
- On group create/join: subscribe to {group}/state and {group}/chat topics
- On group leave: unsubscribe
- On check-in: also auto-subscribe to festival stage chat topic

## Svelte side

### QRCode.svelte
Generate QR from invite payload string. Use a lightweight SVG QR library (install qrcode or generate in Rust). Display in a modal.

### QRScanner.svelte
Camera QR scanner. Use Tauri barcode-scanner plugin or html5-qrcode. On scan: invoke("join_group", { invitePayload }).

### GroupList.svelte
- List groups for current festival
- Each: group name, member count, last activity time
- "Create Group" button → name input → QR display
- Tap group → navigate to group detail

### GroupPresence.svelte
- Member list: avatar (initials circle in avatar_color), display name, current stage (colored indicator + name or "—"), last update ("2m ago")
- Your entry: prominent, with CheckInButton

### CheckInButton.svelte
- Tap → show stage picker (festival stages with color swatches + "Campsite" + custom input)
- On pick: invoke("check_in", { groupId, stageIdOrCustom })
- Brief confirmation animation

### SharedStars.svelte
- Grid/timeline view of group members' starred sets
- Shows overlap/conflicts between members
- "Alex and Sam are both at Four Tet, 20:00"

### PinList.svelte
- List of group pins (campsite, meeting point, etc.)
- Add pin button → label + location input
- invoke("pin_location", { groupId, label, location })

### Routes
- festival/[id]/groups/+page.svelte — group list
- festival/[id]/groups/[groupId]/+page.svelte — group detail (presence + stars + pins + chat entry)

Wire groups into the tab bar or festival detail navigation.

## After implementing
- pnpm check — all pass
- Stack branch: `git spice branch create phase-6-groups` (onto phase-5-ui)
- Commit: `git spice commit create -m "implement passkey auth, friend groups, qr join, presence, shared stars, pins"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, auth flow, group lifecycle, decisions.
```

---

## Phase 7: Chat System

**Goal:** Stage-scoped festival chat and encrypted group chat with DO catch-up.

**Dependencies:** Phase 6

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing chat for OFFBEAT. Phases 1-6 complete. Read existing code — especially gossip_manager.rs and the IPC layer.

## Rust side

### Update gossip_manager.rs
- publish_chat(topic, user_id, text, stage_id: Option) → create ChatMessage, serialize, gossip publish. If group topic: encrypt with group key first.
- On incoming chat gossip: decrypt if group, store in SQLite chat_messages, emit IPC event.
- Auto-subscribe to stage chat topic when user checks in to that stage.

### DO catch-up
- On WebSocket reconnect to festival DO: send catch-up request per topic with last known seq
- DO replays missed messages
- Apply to local SQLite + emit IPC events for any new messages

## Svelte side

### ChatView.svelte
- Props: topic (string), mode ("festival" | "group")
- Message list: scrollable, auto-scroll to bottom on new messages (unless user scrolled up)
- Each message: ChatMessage.svelte
- Stage filter bar (festival mode only): horizontal scroll of stage chips + "ALL". Client-side filter.
- On mount: load history from local DB, listen for chat_message events

### ChatInput.svelte
- Fixed bottom input bar
- Mono-styled input, dotted border
- Send button (accent)
- Stage indicator: if checked in, show "@ STAGE 1" chip next to input
- On send: invoke("send_chat", { topic, text })

### ChatMessage.svelte
- Compact row: timestamp (mono 9px) | display name (bold 13px) | text
- Stage tag: small colored chip if message has stage_id (use stage color at low opacity)
- Own messages: subtle accent-wash background
- Group messages: no stage tag (group context is implicit)

### StageFilter.svelte
- Horizontal scroll of chips: "ALL" + one per stage (with color dot)
- Active chip: accent fill
- Filters messages client-side (just hide/show in DOM)

### Routes
- festival/[id]/chat/+page.svelte — stage-scoped festival chat. Default to "ALL" or auto-select current stage.
- Group chat embedded in group detail page (toggle between presence view and chat)

Wire "NOW" tab to festival chat (it's the natural home during a live festival).

## Design
- Chat feels dense and fast: mono timestamps, compact rows, no avatars (save space)
- Empty state: "// NO MESSAGES" in mono, fg-3
- Stage tags: small, use stage color as chip bg at 20% opacity
- Timestamps: relative ("2m ago") for recent, absolute ("21:34") for older

## After implementing
- pnpm check — all pass
- Stack branch: `git spice branch create phase-7-chat` (onto phase-6-groups)
- Commit: `git spice commit create -m "implement stage chat, group chat, DO catch-up, chat UI"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, chat features, offline behavior, decisions.
```

---

## Phase 8: P2P Transports — BLE + Meshtastic

**Goal:** BLE and Meshtastic as iroh custom transports with fragmentation.

**Dependencies:** Phase 6 (needs groups working)

**Definition of done:** `pnpm turbo check --ui stream --output-logs errors-only`

**Sub-Agent Prompt:**

```
You are implementing BLE and Meshtastic as iroh custom transports for OFFBEAT. Phases 1-6 complete. Read transport/mod.rs and the iroh endpoint setup in lib.rs.

## Background

iroh v1 has a custom transport API behind the `unstable-custom-transports` feature flag. Three traits: CustomTransport, CustomEndpoint, CustomSender. Custom transports participate in QUIC multipath alongside built-in IP and relay. The constraint: QUIC requires minimum 1200-byte MTU. BLE gives ~247 bytes, meshtastic gives ~228 bytes. You MUST implement a fragmentation layer.

Reference: the iroh-tor transport (https://github.com/n0-computer/iroh-tor) is a working example of a custom transport. Study its structure.

## src/transport/fragmentation.rs

Generic fragmentation/reassembly layer:

```rust
const HEADER_SIZE: usize = 4;  // 1 byte packet_id, 1 byte chunk_index, 1 byte total_chunks, 1 byte flags

pub fn fragment(data: &[u8], mtu: usize) -> Vec<Vec<u8>> {
    // Split data into chunks of (mtu - HEADER_SIZE) bytes
    // Prepend header to each chunk
    // Return ordered chunks
}

pub fn reassemble(chunks: &[Vec<u8>]) -> Option<Vec<u8>> {
    // Verify all chunks present (check total_chunks)
    // Strip headers, concatenate in order
    // Return complete payload or None if incomplete
}
```

Include a ReassemblyBuffer that tracks in-flight packets with timeouts.

## src/transport/ble.rs

BLE custom iroh transport using btleplug crate:

Implement CustomTransport, CustomEndpoint, CustomSender traits.

**Discovery:**
- Advertise BLE service with OFFBEAT UUID
- Include group IDs in service data (so peers can discover relevant groups)
- Scan for nearby OFFBEAT devices

**GATT Service:**
- Custom service UUID for OFFBEAT
- Characteristic for sending QUIC packets (write, notify)
- Fragment outgoing QUIC packets (1200+ bytes) into BLE-sized chunks (~247 bytes)
- Reassemble incoming chunks into complete QUIC packets

**Address mapping:**
- iroh custom transports use custom addresses (not IP)
- Map BLE device addresses to iroh CustomAddr

**Transport bias:**
- Register with AddrKind::Custom(0x424C45) (BLE transport ID from iroh's TRANSPORTS.md)

## src/transport/meshtastic.rs

Meshtastic custom iroh transport:

**Connection to hardware:**
- Connect to meshtastic device via BLE GATT (service UUID: 6ba1b218-15a8-461f-9fa8-5dcae273eafd)
- Subscribe to fromRadio characteristic
- Write to toRadio characteristic
- Packets are protobuf-encoded

**QUIC packet wrapping:**
- Wrap fragmented QUIC packets in meshtastic MeshPacket with PortNum::PRIVATE_APP (256)
- Destination: broadcast (0xFFFFFFFF)
- Fragment for 228-byte LoRa payload limit
- Reassemble on receive

**Protobuf:**
- Create proto/meshtastic.proto with minimal subset: MeshPacket, Data, FromRadio, ToRadio, PortNum
- Use prost for codegen (add to build.rs)

## src/transport/mod.rs

- Export BleTransport and MeshtasticTransport
- Both implement iroh's CustomTransport trait
- Register on endpoint builder:

```rust
let endpoint = iroh::Endpoint::builder(presets::N0)
    .custom_transport(BleTransport::new()?)
    .custom_transport(MeshtasticTransport::new()?)
    .transport_bias(AddrKind::Custom(BLE_ID), TransportBias::default())
    .transport_bias(AddrKind::Custom(MESH_ID), TransportBias::default())
    .bind()
    .await?;
```

## UI: Update ConnectionStatus.svelte

Show multi-transport status:
- ● FULL (green dot) — iroh IP/relay connected
- ◐ LOCAL (amber dot) — BLE peers: N
- ◉ MESH (magenta dot) — meshtastic connected
- ○ OFFLINE (grey dot) — cached only

Tap to expand: detail view per transport (connected peers, mesh node, last sync time).

## Tests

- Fragmentation: fragment 2KB payload at 247 MTU → reassemble → matches original
- Fragmentation: fragment 5KB payload at 228 MTU → reassemble → matches original
- Fragmentation: missing chunk → reassemble returns None
- Meshtastic protobuf: encode/decode round-trip

## Important
- Both transports MUST gracefully handle hardware not available. Return error from bind(), log warning, report "unavailable" in transport status. Never panic.
- Only group gossip goes over BLE/mesh. Festival lineup and chat are too large.
- btleplug may need platform-specific features. Use conditional compilation (#[cfg(target_os)]) where needed.

## After implementing
- cargo test — fragmentation + protobuf tests pass
- pnpm check — all pass
- Stack branch: `git spice branch create phase-8-p2p` (onto phase-6-groups)
- Commit: `git spice commit create -m "implement BLE and meshtastic iroh custom transports with fragmentation"`
- Verify: `pnpm turbo check --ui stream --output-logs errors-only`

Report: files created, GATT service design, fragmentation details, meshtastic integration, hardware limitations.
```

---

## Phase 9: Final PR

**Goal:** Open a PR summarizing the full implementation.

**Dependencies:** All phases complete.

Orchestrator opens PR via `gh pr create` summarizing all 8 phases.
