---
{
  "schemaVersion": 1,
  "id": "offbeat.likes-and-personal-schedule",
  "locale": "en",
  "title": "Likes and personal schedule",
  "summary": "Star sets locally, filter your schedule, understand clash markers, and control what joined groups can see.",
  "category": "offbeat",
  "countryCodes": [],
  "aliases": ["stars", "liked sets", "my schedule", "clashes", "group picks"],
  "tags": ["schedule", "stars", "likes", "clashes", "groups"],
  "generatedRefs": [],
  "priority": "normal",
  "order": 840,
  "lastVerified": "2026-08-11",
  "contentStatus": "product-verified",
  "sources": [
    {
      "title": "Personal star persistence",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/crates/core/src/db/mod.rs"
    },
    {
      "title": "Star and group reconciliation API",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/rust/src/api/mod.rs"
    },
    {
      "title": "Mobile schedule wiring",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/main.dart"
    },
    {
      "title": "Schedule overlap model",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/data/models.dart"
    },
    {
      "title": "Group schedule overlay",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/data/group_schedule_overlay.dart"
    },
    {
      "title": "Lineup filtering",
      "publisher": "OFFBEAT",
      "url": "https://github.com/arlyon/offbeat/blob/e54aa7345361a07f31e4e3a11fc8ebd8d079b2d3/apps/mobile/lib/screens/festival_detail/filter_panel.dart"
    }
  ]
}
---
# Likes and personal schedule

Star a set to add it to your personal schedule. Stars are local festival data stored in SQLite, so they do not depend on a live server.

## Current support

**Available:** star and unstar sets, reload saved stars after restart, use `MINE` and `OURS` filters, see supporters from joined groups, and calculate schedule clashes locally.

**Not available:** star history, a user-visible star timestamp, attendance confirmation, automatic conflict resolution, or a delivery/read receipt for group-visible picks.

## Prerequisites

- The festival lineup must already contain the set.
- To see artist and stage details offline, the lineup must be cached.
- To share picks with others, join at least one group for that festival.

## Star and unstar a set

Tap the star on a set row or set-details sheet. OFFBEAT serializes rapid toggles for the same set, writes the personal state to SQLite, then reads it back before updating the screen.

If the write fails, the app restores the durable value where possible and shows `COULD NOT UPDATE MY SCHEDULE`. Try once more after checking available device storage. Repeated tapping is not a sync strategy.

Personal stars remain when you leave a group.

## Schedule views and filters

- `MINE` shows your personal stars.
- `OURS` shows your personal stars plus sets selected by any visible member of your joined groups.
- Supporter markers show other group members who shared the set.
- The `LIKED` view summarizes your starred sets and clashes for each festival day.
- Search filters can limit stage, genre, time window, `MINE`, `OURS`, or hide sets that overlap your starred schedule.

A group pick never silently becomes your personal star.

## How clashes work

A clash is a time overlap on the same festival day. OFFBEAT calculates it from the cached lineup and your current personal stars.

- Cancelled sets are excluded from clash calculations.
- Two of your starred sets are shown as a personal clash when their time intervals overlap.
- An unstarred set may also be marked as clashing when it overlaps one of your stars, which supports the `Hide clashing sets` filter.
- Group-only picks do not drive your personal clash calculation.

Clash Radar is display only. It does not unstar a set, choose a route between stages or account for walking time, capacity changes or delays.

## Offline behavior

Personal stars work without internet and survive app restart in local SQLite. Search, `MINE`, clash calculation and cached lineup views are local.

When you belong to groups, OFFBEAT mirrors your complete star set into each encrypted group document. If no route is available, the local group change is retained and queued for relay retry. Other members may continue to see an older pick list until convergence.

The `OURS` view is also based on the latest group state stored on this device. It can be incomplete or stale while offline.

## Privacy and trust

Without a group, personal stars remain local to the app database.

Inside a group, your set IDs are intentionally shared with members who possess that group's key. The relay can carry encrypted group state but should not see the plaintext set IDs. Group members can associate the picks with your group user ID and display name.

Leaving removes your shared-star entry from the group state and deletes the local group key, but offline members may show the old entry until they receive the leave update. There is no retroactive promise that previously shared information is forgotten.

## Constraints

- A star does not reserve entry, notify the artist, or prove attendance.
- Schedule times are only as current as the cached signed lineup.
- There is no per-change history or timestamp in the current UI.
- Group overlays deduplicate the same identity across multiple local groups, but stale group copies may still produce temporarily different views on different devices.
- No notification or alarm is created merely by starring a set.

## Troubleshooting

### Stars disappeared after opening a festival

Wait for the local star list to load. If the app showed a write error previously, the durable value may differ from the last animation. Do not clear storage, because that would remove local schedule data.

### `OURS` is empty

Confirm you joined a group for this festival and that other members have shared picks. Then allow the group state to catch up through a relay or peer route.

### A supporter appears twice or has an old name

Allow all joined groups to finish syncing. The overlay deduplicates by user identity, but cached group states may temporarily disagree.

### A clash looks wrong

Open both set details and compare day, start time, duration and cancellation state. The calculation uses cached intervals only. Reconnect for a signed lineup update if official times changed.

See [Groups](wiki:offbeat.groups) and [OFFBEAT lineup](wiki:offbeat.lineup).
