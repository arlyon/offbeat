# Festival App Shell

_Started 2026-05-20 19:32 UTC_

---

## User

Please design three different variants for the app shell + top level festival search / favourite screen

## Assistant

_[tool: list_files]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: copy_starter_component]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: update_todos]_

Plan: I'll create three variants of the app shell + festivals/search/favourites screen, presented side-by-side in a design canvas so you can compare. Each takes a distinct stance:

- **A · Index** — by-the-book. Tab bar + dotted search + pinned section.
- **B · Stub Stack** — leans into the tear-off ticket motif. Big duotone stubs, segmented filter, floating dock.
- **C · Console** — most experimental. Mono command bar replaces tabs; type-to-filter table; brutal density.

_[tool: copy_files]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

_[tool: write_file]_

Now the main HTML wiring it all together.

_[tool: write_file]_

_[tool: write_file]_

_[tool: show_html]_

_[tool: done]_

_[tool: fork_verifier_agent]_

Three variants on the canvas — all interactive (star, search, segment/scope filters, tabs all work):

- **A · Index** — conservative. Top nav + bottom tab bar, dotted search field, pinned "SAVED" above "DISCOVER".
- **B · Stub Stack** — the tear-off ticket motif made literal. Big duotone stubs with headliners overlaid, segmented filter, magenta-outlined floating dock instead of a tab bar.
- **C · Console** — terminal density. A `OFFBEAT:fests/>` prompt does double-duty as search + scope, slash-command tabs, a 7-column mono table, F-key status bar at the bottom replacing icon tabs. Densest, most opinionated.

Drag the canvas to pan, scroll inside any phone, or click the expand icon on an artboard to focus it fullscreen. Tell me which direction(s) to push further — happy to mix moves between them (e.g. C's prompt + B's stubs, or A's tab bar with B's segmented filter).

