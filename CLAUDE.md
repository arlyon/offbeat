# CLAUDE.md — OFFBEAT

Festival timeline tracker. Local-first, P2P-native, works without internet.

## Quick Reference

```bash
pnpm check              # Full lint/typecheck (runs turbo check + check:rust)
pnpm check:rust         # Rust only: clippy + tests
cargo test --workspace  # Run Rust tests
pnpm -F @offbeat/server dev  # Start Cloudflare dev server
cd apps/mobile && flutter run # Run Flutter app
```

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           OFFBEAT SYSTEM                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  ┌─────────────────┐         ┌─────────────────────────────────────┐   │
│  │ Cloudflare      │         │ Flutter Mobile Client               │   │
│  │                 │         │                                     │   │
│  │  Main DO        │ REST    │  ┌─────────────┐  ┌──────────────┐  │   │
│  │  • Registry     │◄───────►│  │ Flutter UI  │◄─│ FRB Bridge   │  │   │
│  │  • Auth         │         │  │ (Dart)      │  │ (crates/     │  │   │
│  │                 │         │  └─────────────┘  │  bridge)     │  │   │
│  │  Festival DO    │ WS      │         │         └──────┬───────┘  │   │
│  │  • Gossip relay │◄───────►│         ▼                │          │   │
│  │  • Blind mailbox│         │  ┌──────────────────────────────┐   │   │
│  │  • Lineup signer│         │  │ Rust Core (crates/core)      │   │   │
│  └─────────────────┘         │  │ • iroh (P2P transport)       │   │   │
│                              │  │ • Yrs (CRDTs)                │   │   │
│         ▲                    │  │ • iroh-gossip (pubsub)       │   │   │
│         │                    │  │ • SQLite (persistence)       │   │   │
│         │ P2P                │  └──────────────────────────────┘   │   │
│         │                    │                                     │   │
│         └────────────────────┼──────────► Other peers (BLE/Mesh)   │   │
│                              └─────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────┘
```

## Project Structure

```
offbeat/
├── apps/
│   ├── mobile/              # Flutter app
│   │   ├── lib/             # Dart source
│   │   │   ├── data/        # Mock data (during dev)
│   │   │   ├── screens/     # Screen components
│   │   │   ├── shell/       # App shell (nav, tabs, status)
│   │   │   ├── theme/       # Design tokens + theme
│   │   │   └── widgets/     # Reusable UI components
│   │   └── pubspec.yaml
│   └── server/              # Cloudflare Workers
│       ├── src/
│       │   ├── index.ts     # Entry, Hono router
│       │   ├── main-do.ts   # Main Durable Object
│       │   └── festival-do.ts # Festival Durable Object
│       └── wrangler.toml
├── crates/
│   ├── core/                # Core Rust logic
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs     # Domain types
│   │       ├── doc_manager.rs   # Yrs document management
│   │       ├── gossip_manager.rs # iroh-gossip integration
│   │       ├── crypto.rs    # AES-256-GCM encryption
│   │       ├── signing.rs   # Ed25519 signatures
│   │       ├── topics.rs    # Topic ID derivation
│   │       ├── db/          # SQLite persistence
│   │       └── transport/   # Custom iroh transports
│   └── bridge/              # flutter_rust_bridge FFI
│       └── src/api.rs       # FRB-annotated functions
├── packages/
│   └── protocol/            # Shared TS types + parsers
│       └── src/
│           ├── index.ts
│           └── clashfinder.ts  # Clashfinder JSON parser
├── docs/
│   ├── prd.md              # Full product requirements
│   ├── execution-plan.md   # 8-phase implementation plan
│   └── sync-patterns.md    # Sync strategies
├── Cargo.toml              # Rust workspace
├── turbo.json              # Turborepo config
└── biome.json              # Formatter/linter config
```

## Tech Stack

| Layer | Technology |
|-------|------------|
| Mobile UI | Flutter (Dart) |
| Mobile Core | Rust via flutter_rust_bridge v2 |
| P2P Transport | iroh (QUIC-based, multi-transport) |
| CRDTs | Yrs (Rust port of Yjs) |
| Pub/Sub | iroh-gossip |
| Persistence | SQLite (rusqlite client-side, DO SQLite server-side) |
| Server | Cloudflare Workers + Durable Objects |
| API | Hono (server), REST + WebSocket |
| Auth | WebAuthn passkeys |

## Development Commands

### Root

```bash
pnpm check                  # Full workspace check (lint + typecheck + Rust)
pnpm check:rust             # Rust clippy + tests only
pnpm crap                   # CRAP metric analysis (complexity/coverage)
pnpm crap:ci                # CRAP with threshold check (fails if >30)
```

### Server (`apps/server`)

```bash
pnpm -F @offbeat/server dev         # Local wrangler dev
pnpm -F @offbeat/server check       # TypeScript + Biome
pnpm -F @offbeat/server test        # Vitest
```

Secrets (set via `wrangler secret put`):
- `CLASHFINDER_USERNAME`
- `CLASHFINDER_PRIVATE_KEY`

### Admin Scripts (`apps/server/scripts/`)

All scripts default to `--api-url http://localhost:8787`. Pass `--api-url https://offbeat-server.arlyon.workers.dev` for prod.

All admin-authenticated scripts require `ADMIN_SECRET_KEY` env var (64-char hex Ed25519 secret key).

```bash
# --- Bootstrap & Auth ---
pnpm -F @offbeat/server admin:keygen       # Generate a new Ed25519 keypair (prints to stdout)
pnpm -F @offbeat/server admin:bootstrap    # Bootstrap first admin on a fresh server (no auth needed)
pnpm -F @offbeat/server admin:add          # Add another admin (requires existing admin auth)
  # --generate            Generate a new keypair and register it
  # <public-key>          Or provide an existing 64-char hex public key

# --- Festival Management ---
pnpm -F @offbeat/server festival:register  # Register a single festival from a JSON fixture file
  # <festival.json>       Path to fixture file
  # --dry-run             Validate without making requests

pnpm -F @offbeat/server festival:seed      # Seed all festivals from a folder of JSON fixtures
  # [folder]              Fixture folder (default: fixtures/)
  # --dry-run             Validate without making requests

pnpm -F @offbeat/server festival:delete    # Delete a festival by ID
  # <festival-id>

pnpm -F @offbeat/server festival:reset     # Wipe all Festival DOs + MainDO data, re-seed from fixtures

pnpm -F @offbeat/server festival:announce  # Submit an announcement to a festival's CRDT doc
  # <festival-id> <message>
  # --priority <info|warning|urgent>
  # --title <title>
```

Typical workflow after a fresh deploy or DO wipe:
```bash
export ADMIN_SECRET_KEY=...
npx tsx scripts/bootstrap-admin.ts --api-url https://offbeat-server.arlyon.workers.dev
npx tsx scripts/reset-all.ts --api-url https://offbeat-server.arlyon.workers.dev
```

### Mobile (`apps/mobile`)

```bash
cd apps/mobile
flutter run                  # Run on connected device/emulator
flutter build apk            # Android release
flutter build ios            # iOS release
flutter_rust_bridge_codegen generate  # Regenerate FFI bindings
```

### Rust

```bash
cargo test --workspace       # Run all tests
cargo clippy --workspace --all-targets -- -D warnings  # Lint
cargo doc --workspace --no-deps --open  # Generate docs
```

## Important Rules

- **Never edit generated code.** Files like `frb_generated.rs`, `frb_generated.dart`,
  `frb_generated.io.dart`, `frb_generated.web.dart` are produced by
  `flutter_rust_bridge_codegen generate`. If the Rust bridge API types change, regenerate
  instead of hand-editing.

## Code Conventions

### TypeScript/JavaScript

- **Formatter**: Biome with tabs, 100 char line width
- **Imports**: Auto-organized via Biome
- **Linting**: Biome recommended rules

### Rust

- **Edition**: 2024
- **Lint**: `clippy -D warnings` (treat warnings as errors)
- **Naming**: snake_case for fields, `#[serde(rename_all = "camelCase")]` for JSON interop
- **Errors**: Use `anyhow::Result` for fallible functions

### Flutter/Dart

- **Analysis**: Default rules via `analysis_options.yaml`
- **State**: Use Riverpod or similar for state management
- **Naming**: camelCase for variables, PascalCase for types

### Design System

Dark brutalist aesthetic:
- **Background**: `#0B0B0C`
- **Accent**: `#FF2D8F` (magenta)
- **Fonts**: Helvetica (sans), JetBrains Mono (mono)
- **Borders**: 1.5px dotted, zero border-radius everywhere
- **Motion**: 80-380ms, `cubic-bezier(0.2, 0.7, 0.2, 1)`
- **Tap targets**: minimum 44px

See `apps/mobile/lib/theme/tokens.dart` for full token definitions.

## Key Architectural Patterns

### Data Fetching — iroh-gossip Only

**All festival/lineup data flows through iroh-gossip and the Rust core. The Flutter
UI never fetches lineup data via REST.** The Rust node connects to the Festival DO's
WebSocket relay, subscribes to gossip topics, receives Yrs CRDT updates, persists
them to local SQLite, and exposes them to Flutter via FRB. The only REST calls from
Flutter are: festival list discovery (`GET /festivals`), auth endpoints, and admin
endpoints. Never add REST-based lineup/data fetching as a workaround — fix the
gossip pipeline instead.

### CRDTs for State

All shared state uses Yrs documents:
- **Festival state**: Lineup, stages, sets, announcements (signed by festival DO)
- **Group state**: Members, presence, stars, pins (encrypted with group key)

Updates sync via iroh-gossip. Late joiners use state vector diffs for fast-forward.

### Topic ID Derivation

```rust
// Public festival topics
blake3("offbeat/{festival_id}/{channel}")

// Private group topics
blake3(group_key || channel)
```

Channels: `state`, `chat/{stage_id}`, `chat` (for groups)

### Encryption

- **Lineup updates**: Ed25519 signed by festival DO
- **Group data**: AES-256-GCM encrypted with 256-bit group key
- **Key derivation**: Group ID = blake3(group_key) first 16 bytes hex

### Transport Hierarchy

iroh selects best available transport automatically:
1. Internet (relay/direct) — full connectivity
2. WiFi Direct — local high-speed
3. BLE — proximity sync (~10m)
4. Meshtastic — long-range mesh (~3km)

## Data Flow

### Festival Lineup Update

```
Festival DO signs Yrs update
       │
       ▼
Gossip broadcast on fest/{id}/state
       │
       ├──► Client A (via WS relay)
       │         │
       │         ▼
       │    Verify signature → Apply to local Yrs doc → Persist to SQLite
       │
       └──► Client B (via P2P)
                 │
                 ▼
            Relay to Client C (trustless relay, trusted origin)
```

### Group Presence Update

```
User checks in to stage
       │
       ▼
Update group Yrs doc (presence map)
       │
       ▼
Encrypt update with group key
       │
       ▼
Gossip broadcast on group/{id}/state
       │
       ▼
Group members decrypt → Apply → Update UI
```

## Testing

### Rust Tests

```bash
cargo test --workspace              # All tests
cargo test -p offbeat-core          # Core crate only
cargo test doc_manager              # Filter by name
```

Key test areas:
- `doc_manager`: Yrs create/mutate/persist/reload, signed update verification
- `crypto`: Encrypt/decrypt round-trip, deterministic key derivation
- `topics`: Deterministic topic ID generation

### Server Tests

```bash
pnpm -F @offbeat/server test
```

Uses Vitest. Tests should cover API routes and DO behavior.

### Integration Testing

Manual two-device tests for sync scenarios:
1. Sync over IP → disable IP → sync over WiFi Direct → disable WiFi → sync over BLE

## Performance Targets

| Metric | Target |
|--------|--------|
| Offline usability | 100% core features |
| Cold start (cached) | <2s to interactive |
| Late join (day 4) | <5s to interactive |
| Group join (QR → synced) | <3s on WiFi |
| Gantt scroll perf | 60fps |
| BLE sync (check-in) | <5s |
| Mesh sync (check-in) | <30s |

## Common Tasks

### Adding a New IPC Command

1. Define command in `crates/core/src/types.rs` (if needed)
2. Implement in `crates/core/src/*.rs`
3. Add FRB-annotated wrapper in `crates/bridge/src/api.rs`
4. Run `flutter_rust_bridge_codegen generate`
5. Call from Flutter via generated bindings

### Adding a New Gossip Topic

1. Add topic derivation function in `crates/core/src/topics.rs`
2. Update `gossip_manager.rs` subscription logic
3. Add message handling in dispatch loop
4. Update UI to subscribe/display

### Adding a New UI Screen

1. Create screen in `apps/mobile/lib/screens/`
2. Add route in navigation
3. Wire to Rust core via FRB streams/commands
4. Follow design tokens from `lib/theme/`

## Troubleshooting

### FRB codegen fails

```bash
cd apps/mobile
flutter_rust_bridge_codegen generate --clean
```

### Rust compilation issues

```bash
cargo clean
cargo build --workspace
```

### Wrangler dev issues

```bash
# Check secrets are set
wrangler secret list

# Check wrangler.toml syntax
wrangler deploy --dry-run
```

## Documentation

- **PRD**: `docs/prd.md` — Full product requirements with data schemas
- **Execution Plan**: `docs/execution-plan.md` — 8-phase implementation
- **Sync Patterns**: `docs/sync-patterns.md` — Detailed sync strategies
- **Designs**: `docs/designs/` — UI mockups and design bundles

## Branch Strategy

Using git-spice for stacked branches:
- Each phase creates a stacked branch: `phase-{n}-{description}`
- All branches must pass `pnpm check` before completion
- Definition of done: `pnpm turbo check --ui stream --output-logs errors-only`
