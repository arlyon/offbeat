# OFFBEAT offline field guide

The field guide is authored as a restricted GitHub-flavoured Markdown corpus and compiled into one read-only Flutter asset. Runtime use does not require a server, browser, CMS or network connection.

## Layout

```text
assets/wiki/
  pages/<locale>/<category>/<page>.md       Canonical editorial pages
  generated/psychonautwiki/<substance>.json Explicit imported records
  index.json                                Generated runtime corpus
```

`index.json` is derived and checked in so Flutter can bundle one deterministic asset. Never edit it by hand.

## Page format

Every page starts with `---`, a strict JSON object, another `---`, and a Markdown body beginning with one level-one heading. JSON is deliberately used as a constrained, deterministic subset of YAML front matter.

Required fields:

- `schemaVersion`: currently `1`.
- `id`: stable lowercase identifier, for example `emergency.get-help`.
- `locale`: BCP 47 language or language-country tag used by the corpus.
- `title` and `summary`: concise user-facing strings.
- `category`: `emergency`, `campsite`, `mobility`, `drug-testing`, `substances`, `meshtastic` or `offbeat`.
- `countryCodes`: ISO alpha-2 jurisdictions, or an empty list for universal pages.
- `aliases` and `tags`: offline search terms.
- `generatedRefs`: imported-record IDs rendered below the editorial body.
- `priority`: `critical`, `high` or `normal`.
- `order`: stable non-negative display order.
- `lastVerified`: ISO date when claims and sources were last checked.
- `contentStatus`: `source-checked`, `product-verified` or `imported-unreviewed`.
- `sources`: one or more source objects with `title`, `publisher` and HTTPS `url`; optional `revision` and `license` fields.

Use `[another page](wiki:stable.page-id)` for internal links. HTTPS links are allowed but are optional enhancements because the article itself must remain useful offline. Raw HTML, executable content, network images and other URI schemes are rejected.

## Safety content rules

- Start with observable danger signs and immediate actions.
- Link emergencies to `emergency.get-help` rather than duplicating or weakening it.
- Do not diagnose, declare a dose or combination safe, provide personalised calculations, or delay escalation while identifying a substance.
- Separate authoritative emergency guidance from community metadata.
- Mark limited or conflicting evidence in the article itself.
- Do not turn an unsupported supplement, detox or neuroprotection claim into advice.
- Country-specific emergency numbers and services belong only in the matching country pack.

`generatedRefs` expose imported dose and duration labels in a fixed warning treatment. Values, units, route names and categories are displayed verbatim from the selected PsychonautWiki semantic-data fields; OFFBEAT does not reinterpret them. Each generated section identifies the source page, exact revision and the upstream CC BY 4.0 semantic-data licence. These source labels are not safe-dose advice, reviewed clinical recommendations, calculators or publication authority for editorial prose.

## Commands

From `apps/mobile`:

```bash
# Regenerate the checked-in runtime corpus
dart run tool/build_wiki.dart

# Validate source files and fail if index.json is stale
dart run tool/build_wiki.dart --check

# Explicitly refresh all selected PsychonautWiki records
dart run tool/import_psychonautwiki.dart
dart run tool/build_wiki.dart

# Refresh one record
dart run tool/import_psychonautwiki.dart --only mdma
dart run tool/build_wiki.dart
```

The importer records the GraphQL query, exact MediaWiki revision, retrieval time, source-payload SHA-256, generator version and content licence. Live imports hash the exact GraphQL response bytes received; fixture imports hash the fixture bytes. Inspect every import diff before accepting it. Regeneration must never overwrite editorial Markdown.

For deterministic importer tests, pass a versioned fixture with `--fixture` and a temporary `--output` directory. Normal tests and app startup must never contact PsychonautWiki.

## Updating a page

1. Confirm the jurisdiction and current authoritative sources.
2. Edit the canonical Markdown, preserving the stable ID.
3. Update `lastVerified` only after checking every consequential claim and source.
4. Run the compiler and inspect the source and generated diffs.
5. Run targeted Flutter tests and `flutter analyze`.
6. Roll back by reverting the page/import commit and regenerating the index.

PsychonautWiki API software licensing does not relicense returned content. PsychonautWiki's Copyrights page, revision 145652, declares semantic data under CC BY 4.0 and most other text and metadata under CC BY-SA 4.0. The current importer uses only GraphQL semantic fields and attributes them as CC BY 4.0 in every generated section. Images and prose require separate rights review and are intentionally excluded; any future adapted prose must carry its applicable attribution and ShareAlike notice.
