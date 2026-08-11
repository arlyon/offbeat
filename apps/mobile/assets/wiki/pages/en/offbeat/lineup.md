---
{
  "schemaVersion": 1,
  "id": "offbeat.lineup",
  "locale": "en",
  "title": "OFFBEAT lineup",
  "summary": "Use a previously synced festival lineup, search and filter it offline, and understand freshness and trust limits.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["festival schedule", "sets", "stages", "lineup search", "cancellations"],
  "tags": ["lineup", "offline", "schedule", "signed data", "search"],
  "generatedRefs": [],
  "priority": "high",
  "order": 810,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Mobile lineup and weather wiring",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/main.dart"
    },
    {
      "title": "Rust mobile API",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/mod.rs"
    },
    {
      "title": "Lineup document decoding",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/dto.rs"
    },
    {
      "title": "Signed CRDT document management",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/doc_manager.rs"
    },
    {
      "title": "Festival registry service",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/services/festival_service.dart"
    },
    {
      "title": "Offline lineup search",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/screens/festival_detail/lineup_search_screen.dart"
    }
  ]
}
---
# OFFBEAT lineup

The lineup shows festival days, stages and sets from OFFBEAT's local festival document. You can browse day and stage views, open set details, search by artist, stage or genre, and filter the copy already on the device.

## Current support

**Available in the normal mobile UI:** cached lineup display, day and stage views, the liked-set clash view, local search and filters, set cancellation flags, and live updates when a valid sync route delivers them.

**Not currently available as a normal attendee feature:** a manual force-refresh control, a lineup age or "latest" guarantee, and festival announcements in the mobile lineup UI. Admin-only Clashfinder controls are not an attendee refresh path.

## Prerequisites

- OFFBEAT must already know the festival from the server-authoritative festival registry or its local registry cache.
- The festival's signing key must be configured before new festival-state updates are accepted.
- At least one valid lineup copy must have synced before it can be used offline.

A fresh installation with no internet and no cached registry cannot discover an unseen festival from nearby peers. Festival discovery uses the registry endpoint; lineup data itself does not use a Flutter REST lineup request.

## How lineup data arrives

When you open a festival, OFFBEAT reads its local Yrs document immediately and starts watchers for changes. It then tries the Festival Durable Object relay and registered peer routes. Festival state includes stages, days, sets, cancellations and weather.

The Flutter lineup reads only through the Rust core. Incoming festival checkpoints and deltas must carry a valid festival-authority signature, the expected document identity and a non-rollback authority sequence before they are applied and persisted. The relay or a nearby peer transports data but is not the lineup authority.

## Offline behavior

After a valid copy has been stored, these actions are local and do not need a connection:

- Browse days and stages.
- Open set details.
- Search artists, stages and genres.
- Filter by stage, genre, time window, personal picks, group picks or clashes.
- See locally stored cancellations.
- Star or unstar a set in your personal schedule.

The app keeps the accepted Yrs document and incremental updates in SQLite, so the lineup can survive an app restart. Offline means "use the last accepted copy", not "the copy is current".

## Cancellations and freshness

A cancelled set is marked `CANCELLED` in set details and is excluded from live-now and clash calculations. That status is only as fresh as the local signed festival document.

OFFBEAT currently does not show a dedicated lineup `updated at` time. A relay or Bluetooth `ONLINE` label means a transport is running, not that the latest lineup has arrived. Message counts and peer counts in the connection drawer are diagnostic traffic indicators, not proof of lineup freshness.

If venue screens, stewards or official event messages disagree with the cached lineup, follow the venue's current information.

## Privacy and trust

The public festival lineup is authority-signed, not secret. Nearby peers and relay infrastructure may carry its encrypted transport frames or public signed envelopes depending on route, but only valid authority-signed festival state is accepted.

Your personal stars are stored separately from the public lineup. They stay local unless you are in a group, in which case OFFBEAT mirrors your picks into that group's encrypted state. See [Likes and personal schedule](wiki:offbeat.likes-and-personal-schedule).

## Constraints

- OFFBEAT cannot invent a missing first lineup while fully offline.
- Search and filters operate only on sets already stored locally.
- A visible transport does not guarantee immediate convergence.
- Announcements are planned in the wider festival-state model but are unavailable in the current attendee UI.
- Weather is separate from the set schedule. See [OFFBEAT weather](wiki:offbeat.weather).

## Troubleshooting

### The festival list is empty

Reconnect long enough to load the server festival registry. If this device had a previous successful registry fetch, reopen the app and look for the cached-copy status. Do not delete app storage as a first troubleshooting step.

### The festival opens with `NO LINEUP DATA`

The local festival document has no complete stages, days or sets yet. Leave the festival selected, check the connection drawer, and allow the relay or a nearby peer time to catch up. Confirm you selected the intended festival.

### Search returns no matches

Clear active filters and check spelling. Search only covers the locally available artist, stage and genre text.

### A set time looks old

Treat the display as cached until a current official source confirms it. Re-establish a route and reopen the festival. If the venue has announced a change that OFFBEAT does not show, use the venue information.

For how routes and convergence work, see [P2P syncing](wiki:offbeat.p2p-syncing).
