# Festival View Variants

_Started 2026-05-20 19:33 UTC_

---

## User

Please design different variants for the festival view page. I'd like to explore different views

- gantt view. as you scroll vertically it should dynamically adjust the start and end so it feels like we are scrolling horizontally
- top tabs by day for multi-day festivals
- top tabs by stage
- nice looking filters
- some more UIs at your discretion

## Assistant

_[tool: list_files]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: read_file]_

_[tool: update_todos]_

I have rich context from the OFFBEAT design system. The user has been specific — let me skip the question round and explore. I'll use the design_canvas to lay out multiple festival-view variants side-by-side: gantt-scroll, day-tabs, stage-tabs, filter panel, plus a couple at my discretion.

_[tool: copy_files]_

_[tool: copy_starter_component]_

_[tool: grep]_

_[tool: write_file]_

Now I'll plan the data, shared styles, and shared components, then build each variant.

_[tool: write_file]_

_[tool: write_file]_

Now the shared components file:

_[tool: write_file]_

Now the variants file — this is the meat. I'll start with the gantt-scroll since it's the signature ask.

_[tool: write_file]_

Now the main HTML file that wires it all into a design canvas:

_[tool: write_file]_

_[tool: show_html]_

_[tool: done]_

_[tool: save_screenshot]_

_[tool: view_image]_

_[tool: str_replace_edit]_

I see issues: the caption is absolute-positioned at phone bottom, overlapping the gantt's stage rows. Let me fix:

_[tool: save_screenshot]_

_[tool: view_image]_

