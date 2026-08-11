# Offline wiki/content formats for Offbeat's Flutter safety guide

**Research date:** 2026-08-10

**Scope:** Open, author-friendly formats for an offline-first Flutter safety guide containing generated PsychonautWiki substance metadata and hand-authored clinical content. Package maintenance observations are point-in-time, based on primary package registries and repositories.

## Executive recommendation

Use **GFM-like Markdown with strictly validated YAML front matter for editorial pages, generated JSON sidecars for PsychonautWiki data, and a build-generated SQLite/FTS5 read model**.

- Keep hand-authored clinical prose in small, reviewable UTF-8 Markdown files.
- Keep generated substance records out of those prose files. Commit normalized JSON sidecars with explicit source, source revision/retrieval time, license, raw-content hash, and generator version.
- Compile both sources into a read-only, versioned SQLite database. Offbeat already uses `rusqlite` in the Rust core (`crates/core/Cargo.toml`), so search and content lookup should stay in Rust and cross the existing FRB boundary; do not add a second Dart persistence stack.
- Render only the Markdown body in Flutter with [`flutter_markdown_plus`](https://pub.dev/packages/flutter_markdown_plus), using application-owned link and image resolvers. It is a renderer, not a database, search engine, CMS, or provenance system.
- Use SQLite [FTS5](https://www.sqlite.org/fts5.html) for offline title, alias, tag, and body search. Test tokenization per locale.

This is the smallest option that is pleasant to author, easy to review in Git, native-feeling in Flutter, deterministic enough to verify in CI, and explicit about the safety-critical boundary between imported facts and clinician-reviewed guidance.

Do **not** use MediaWiki XML, OpenZIM, TiddlyWiki, AsciiDoc, or EPUB as Offbeat's canonical/runtime format. Each is valuable in its intended role, but each adds a server/parser/WebView/native-reader toolchain without solving Offbeat's clinical provenance model. OpenZIM and EPUB remain reasonable *export artifacts* if external distribution later becomes a requirement.

## How to read the assessment

Three separate concerns are easy to conflate:

1. **Authoring format** — what people edit and review.
2. **Delivery/storage format** — what is bundled in the application.
3. **Rendering/search/content management** — runtime capabilities supplied by libraries and application code.

A Markdown renderer does not provide storage or search. A ZIM or EPUB reader does not provide an authoring workflow. MediaWiki and TiddlyWiki are content-management runtimes, not merely text syntaxes.

Also, an implementation's software license does not license the content processed by it. For example, PsychonautWiki's Bifrost API implementation is [MIT-licensed](https://github.com/psychonautwiki/bifrost/blob/master/LICENSE), while PsychonautWiki's MediaWiki `siteinfo` currently declares its content rights as **Creative Commons Attribution-ShareAlike 4.0 International** ([live `rightsinfo` response](https://psychonautwiki.org/w/api.php?action=query&meta=siteinfo&siprop=general%7Crightsinfo&format=json&formatversion=2)). Offbeat must track content and image licenses independently of API/tool licenses.

## Comparison at a glance

Ratings are relative to this use case, not general judgments about the formats.

| Candidate | Open specification / implementation | Author experience | Structured metadata | Offline closure and reproducibility | Search | Localization | Flutter fit | Offbeat verdict |
|---|---|---|---|---|---|---|---|---|
| **CommonMark/GFM Markdown + validated front matter** | Strong: precise CommonMark and GFM specs; broad BSD-licensed tooling | Excellent; plain text, Git-native, ubiquitous editors | Strong only when Offbeat defines and validates a schema; front matter itself is only a convention | Strong; local links/assets and normalized text are easy to enumerate, hash, and bundle | No native index; excellent when compiled to FTS5 | Convention-based but simple: stable page ID plus BCP 47 locale | Excellent native widget support | **Canonical authoring format** |
| **MediaWiki wikitext + XML export** | Open-source MediaWiki; XML export has an XSD, but practical wikitext semantics depend on MediaWiki, templates, and extensions | Excellent inside a running wiki; poor in ordinary Git review | Rich in a running wiki through templates/categories/extensions; export preserves content model and revisions | Weak-to-medium; XML is not a rendered, self-contained site and template/media closure requires extra work | Server feature; not embedded in the export | Strong in MediaWiki workflows; mappings are not a simple portable content bundle | No credible native wikitext renderer; custom parser/server or HTML/WebView path required | **Import source only, not app format** |
| **OpenZIM** | Open documented format; reference `libzim` is GPL-2.0-or-later and tools are GPL-3.0-or-later | Weak; it packages already-produced HTML/resources | Good archive-level metadata; page-level semantics remain in HTML/application conventions | Excellent single-file closure; reproducible bytes require controlling UUID/date/order/toolchain | Excellent when the optional Xapian index is present | Archive language metadata; often one archive per language | Poor: no maintained Dart reader found; native `libzim` integration and license review needed | **Possible export/delivery format, excessive for v1** |
| **TiddlyWiki** | BSD-3-Clause implementation and documented tiddler formats | Good for individual/browser editing; plugin/macros create dialect coupling | Flexible string-valued tiddler fields | Excellent single-HTML-file option; generated timestamps/plugin state need normalization for reproducibility | Built-in runtime field search, not a portable indexed search layer | UI language/plugin ecosystem; content translation is application convention | WebView/JavaScript runtime, not a native Flutter renderer | **Too much runtime for the requirement** |
| **AsciiDoc** | Open Eclipse specification effort, currently incubating; mature MIT-licensed Asciidoctor implementation | Excellent for complex technical books, includes, attributes, admonitions, cross-references | Strong document header and custom attributes | Strong if includes and resources are vendored and processor versions pinned | No standard embedded full-text index | Built-in labels can be localized; translated prose still needs a workflow | No maintained full AsciiDoc Dart renderer found; precompile to HTML or implement a parser | **Good authoring language, poor Flutter maintenance fit** |
| **EPUB 3.3** | W3C Recommendation with royalty-free implementation commitments | Good publishing ecosystem, but EPUB itself is XHTML/CSS/XML-in-ZIP rather than friendly source | Excellent publication metadata; page/entity schemas need additional conventions | Excellent single-file package; deterministic ZIPs require fixed order/timestamps/toolchain | Reader feature; EPUB does not require a portable full-text index | Strong `dc:language`/`xml:lang` support; commonly one publication per language | Several readers exist, but all are much heavier than a Markdown widget | **Good export, wrong internal model** |
| **Static HTML5 + JSON manifest** | Open Living Standard and ubiquitous tools | Medium-to-poor for direct clinical authoring; usually generated from another source | Strong with JSON/JSON-LD/custom manifest | Strong if all resources are local and a deterministic archive is built | No standard index | Strong HTML language primitives | WebView or HTML-to-widget renderer; larger security/styling surface | **Useful generated interchange, not canonical source** |

## Detailed assessment

### 1. CommonMark/GFM Markdown with front matter

#### Openness and authoring

CommonMark is a precise, test-backed Markdown specification; its repository describes the language as a rationalized Markdown with a specification and BSD-licensed reference implementations. The specification text is CC BY-SA 4.0 and its test software is BSD-licensed ([CommonMark repository](https://github.com/commonmark/commonmark-spec), [license](https://github.com/commonmark/commonmark-spec/blob/master/LICENSE)). GitHub Flavored Markdown is a documented CommonMark superset adding tables, task-list items, strikethrough, and autolinks ([GFM specification](https://github.github.com/gfm/)).

This is the strongest authoring choice for a mixed editorial/engineering team: source diffs are readable, editors are abundant, and content can remain valid without a specialized server. GFM tables are useful for concise safety information, but critical warnings should be represented as typed application data/UI rather than relying on non-standard admonition syntax.

#### Metadata/schema

YAML front matter is not part of CommonMark or GFM. It is an ecosystem convention: Jekyll, for example, defines YAML between leading `---` delimiters and permits arbitrary custom fields ([Jekyll front matter](https://jekyllrb.com/docs/front-matter/)). YAML itself has an open 1.2.2 specification and is Unicode-based ([YAML 1.2.2](https://yaml.org/spec/1.2.2/)).

That flexibility is useful but unsafe unless Offbeat supplies the missing contract. The build must enforce a versioned JSON Schema or equivalent typed validator, reject unknown fields, normalize date/locale/URI forms, and bound nesting/size. Parse front matter at build time, not on every mobile render.

Nested generated pharmacological data *can* fit in YAML, but should not. Machine rewrites of the same file that holds reviewed prose cause noisy diffs and can silently invalidate the review. Generated JSON sidecars give imported data a separate lifecycle and provenance chain.

#### Links, images, bundling, and search

CommonMark/GFM supports links and images, but does not define a multi-file package. Offbeat should define only relative internal page IDs and content-addressed local assets. The compiler can enumerate every dependency, reject missing or remote images, normalize UTF-8/LF, stable-sort records, and hash the resulting bundle.

Markdown itself has no search index. This is an advantage here: use the application's existing database layer rather than coupling search behavior to a document renderer. SQLite's FTS5 module supports phrases, prefixes, NEAR queries, column filters, tokenizers, BM25 ranking, snippets, and integrity/optimization commands ([FTS5 documentation](https://www.sqlite.org/fts5.html)). SQLite also explicitly documents a database with a defined schema as an application file format ([SQLite application file format](https://www.sqlite.org/appfileformat.html)).

#### Attribution and localization

Neither Markdown nor front matter provides a standard provenance model. Offbeat can define a better one than the alternatives: per source, record canonical URL, title, contributor/organization, license, source revision, retrieval time, content hash, transformation version, and whether a displayed field is imported, transformed, or editorial.

Localization is likewise an application convention. Use one stable content ID across locales, store the BCP 47 locale separately, and make fallback explicit in UI. This is simpler and more testable than hiding translation state in prose or wiki templates.

#### Flutter support

The Dart [`markdown`](https://pub.dev/packages/markdown) package parses Markdown to an AST/HTML and offers CommonMark/GFM-like extension sets, but its own README says it provides no HTML sanitization ([repository README](https://github.com/dart-lang/tools/blob/main/pkgs/markdown/README.md)). It is a **parser**, not a Flutter renderer, database, search engine, or CMS.

[`flutter_markdown_plus`](https://pub.dev/packages/flutter_markdown_plus) renders Markdown into Flutter widgets, defaults to GFM, supports tables, task lists, links, and local/asset/network images, and intentionally does not support inline HTML ([package README](https://github.com/foresightmobile/flutter_markdown_plus/blob/main/README.md)). That last limitation is beneficial for a controlled safety corpus: prohibit raw HTML and use explicit custom builders only for reviewed application components.

### 2. MediaWiki wikitext and XML export

#### Strengths

MediaWiki is GPL-2.0-or-later open-source software ([MediaWiki `COPYING`](https://github.com/wikimedia/mediawiki/blob/master/COPYING)). Its authoring UI, revision history, templates, categories, internal links, images, and translation extensions are proven at very large scale. MediaWiki documents its wikitext formatting, links, and image syntax ([formatting](https://www.mediawiki.org/wiki/Help:Formatting), [links](https://www.mediawiki.org/wiki/Help:Links), [images](https://www.mediawiki.org/wiki/Help:Images)).

The XML export is substantially more rigorous than a loose scrape. The current XSD models site information, page IDs, namespaces, redirects, revision IDs, timestamps, contributors, comments, content models/formats, content text, SHA-1 values, and upload metadata ([export XSD 0.11](https://www.mediawiki.org/xml/export-0.11.xsd)). Full-history export is specifically intended to preserve authorship information and attribution ([Help:Export](https://www.mediawiki.org/wiki/Help:Export)). MediaWiki's ContentHandler also supports content models beyond wikitext, including JSON and plain text ([ContentHandler](https://www.mediawiki.org/wiki/Manual:ContentHandler)).

#### Why it is not an app bundle

The XSD standardizes the export envelope, not a standalone rendered wiki. The export contains serialized wikitext; it is explicitly not an XML rendering of wiki markup. Correct rendering may depend on recursively exported templates, parser functions, extensions, site configuration, and CSS. Upload elements carry metadata/source references, not a guaranteed self-contained binary media corpus. Search indexes are not part of the page XML.

A complete MediaWiki authoring system would also duplicate Git review, deployment, authentication, and content-release workflows. A partial wikitext parser would be worse: PsychonautWiki's template/extension dialect could render incorrectly in safety-critical pages.

For PsychonautWiki ingestion, use its typed API as an upstream data source, not wikitext as Offbeat's canonical format. Bifrost's own README says it fetches substance data from the MediaWiki API and exposes a typed GraphQL interface with stale-while-revalidate caching ([Bifrost README](https://github.com/psychonautwiki/bifrost/blob/master/README.md)). Because its schema exposes substance fields but not a source revision for each result, the ingest job should separately resolve and record the relevant MediaWiki page revision where possible.

#### Flutter support

The currently published [`wikimedia_dart`](https://pub.dev/packages/wikimedia_dart) package is an independent REST API client, not an official Wikimedia library and not a wikitext renderer or offline store ([package README](https://github.com/Zaidusyy/wikimedia_dart/blob/main/README.md)). It also does not solve offline rendering of PsychonautWiki-specific templates. No maintained native Flutter package was found that provides a compatible MediaWiki parser plus template/extension environment.

### 3. OpenZIM

#### Strengths

ZIM is designed for compressed offline web content. Its format documents a versioned binary header, UUID, entries, clusters, namespaces, and checksum ([ZIM file format](https://wiki.openzim.org/wiki/ZIM_file_format)). Article entries are complete UTF-8 HTML pages with relative links to packaged CSS, images, scripts, fonts, and other resources ([Article Format](https://wiki.openzim.org/wiki/Article_Format)). `zimwriterfs` packages a self-sufficient HTML directory into one compressed ZIM file ([zim-tools README](https://github.com/openzim/zim-tools/blob/main/README.md)).

ZIM also has a useful standardized archive metadata set: stable name, title, creator, publisher, date, description, ISO 639-3 language, license, source, scraper, and tags ([OpenZIM metadata](https://wiki.openzim.org/wiki/Metadata)). Optional Xapian full-text and title indexes have designated paths in the archive, and `libzim` exposes search APIs when compiled with Xapian ([search-index format](https://wiki.openzim.org/wiki/Search_indexes), [`libzim` usage](https://libzim.readthedocs.io/en/latest/usage.html#full-text-searching-in-entries)).

#### Costs for Offbeat

OpenZIM is a delivery format, not a clinical authoring language. Offbeat would first have to render its source to a self-contained HTML site, then package and read that site. Archive metadata is strong at the corpus level but does not replace page/field-level clinical provenance.

Single-file closure is excellent. Bit-for-bit reproducibility is not automatic: the format includes a UUID and checksum, standard metadata includes a creation date, and compression/index output depends on pinned tool versions and stable input order. A deterministic pipeline is possible but must be engineered and tested.

The reference [`libzim`](https://github.com/openzim/libzim) implementation is GPL-2.0-or-later; `zim-tools` is GPL-3.0-or-later. Embedding/linking it in a mobile application therefore requires dependency and license review in addition to native build work.

#### Flutter support

No maintained OpenZIM reader was found on pub.dev. The package named [`zim`](https://pub.dev/packages/zim) is unrelated Zego instant messaging and is marked discontinued/replaced; it is not an OpenZIM library. A Flutter implementation would require a custom FFI/platform bridge to `libzim`, or a local HTTP/WebView reader. Both are disproportionate for a small safety guide.

### 4. TiddlyWiki

TiddlyWiki is an open-source, browser-based, non-linear notebook. Its current site describes content as small titled “tiddlers” connected by links, tags, lists, and macros, using TiddlyWiki's WikiText syntax ([TiddlyWiki](https://tiddlywiki.com/)). The implementation is BSD-3-Clause, and the project explicitly states that users retain rights in their own content ([license](https://tiddlywiki.com/static/License.html)).

Tiddler storage is unusually flexible: `.tid` files use `name:value` headers plus body text; JSON files are arrays of string-valued property maps; and modern single-file HTML stores JSON tiddlers in a script tag ([TiddlerFiles](https://tiddlywiki.com/static/TiddlerFiles.html)). Standard fields include title, text, created/modified, creator/modifier, tags, type, list, description, and source, and arbitrary extra fields are permitted ([TiddlerFields](https://tiddlywiki.com/static/TiddlerFields.html)). Built-in filter syntax includes field/text search ([search operator](https://tiddlywiki.com/static/search%2520Operator.html)). The live project reported version 5.4.1 at research time, indicating active maintenance.

Those are genuine strengths for a personal knowledge base. For Offbeat, however:

- its schema is flexible rather than typed;
- provenance fields and review state would still be custom;
- browser search is not a reusable native mobile FTS index;
- macros/plugins couple content to the TiddlyWiki JavaScript runtime;
- deterministic output requires suppressing or normalizing modified timestamps and plugin/build state; and
- Flutter would host the whole application in a WebView rather than render content with native semantics and Offbeat styling.

[`webview_flutter`](https://pub.dev/packages/webview_flutter) is maintained by the Flutter project and wraps Android WebView and Apple WKWebView ([README](https://github.com/flutter/packages/tree/main/packages/webview_flutter/webview_flutter)), but it is only a WebView. It does not turn TiddlyWiki into native Flutter content or provide Offbeat's storage, search, attribution, or safety model.

### 5. AsciiDoc

AsciiDoc is a semantic plain-text language with headers, custom document attributes, cross-references, images, tables, admonitions, includes, conditionals, and reusable content. The official site emphasizes plain-text authoring, version control, semantic elements, modularization, and multiple outputs ([AsciiDoc](https://asciidoc.org/)); Asciidoctor converts it to HTML5, DocBook, man pages, PDF, EPUB, and other formats ([Asciidoctor repository](https://github.com/asciidoctor/asciidoctor)). This is a better language than Markdown for a large technical manual.

The standardization story is improving rather than finished. The Eclipse AsciiDoc Language project describes an open specification and APIs, but its project state is **Incubating** and lists EPL-2.0 for project content ([Eclipse project](https://projects.eclipse.org/projects/asciidoc.asciidoc-lang)). The mature Asciidoctor implementation is MIT-licensed.

Metadata can be expressed through the document header, author/revision fields, and custom attributes ([document header reference](https://docs.asciidoctor.org/asciidoc/latest/document/header-ref/)). Links and images are first-class ([link macros](https://docs.asciidoctor.org/asciidoc/latest/macros/links/), [images](https://docs.asciidoctor.org/asciidoc/latest/macros/images/)). Reproducible offline builds are feasible if includes are local, URI reads are disabled, assets are vendored, timestamps are controlled, and the processor/extensions are pinned. Search still needs an external index, and clinical provenance still needs an Offbeat schema.

The decisive weakness is Flutter support. No maintained full AsciiDoc parser/renderer was found on pub.dev. [`rimu`](https://pub.dev/packages/rimu), the top relevant search result, explicitly describes itself as a separate markup language *inspired by* AsciiDoc and Markdown and was last published in 2022; it is not an AsciiDoc compatibility layer. Offbeat would have to precompile to HTML and render through WebView/HTML widgets, or maintain a parser. That extra conversion/runtime seam is not justified by the guide's expected complexity.

### 6. EPUB 3

EPUB 3.3 is a current W3C Recommendation and a standardized single-file distribution format for semantically enhanced web content: XHTML/HTML, CSS, SVG, images, fonts, and other resources in an OCF ZIP container ([EPUB 3.3](https://www.w3.org/TR/epub-33/)). W3C states that Recommendation-track implementations have royalty-free commitments.

EPUB has excellent publication-level metadata: required identifier/title/language, optional creator/contributor/date/subject/type, extensible `meta` properties, a manifest, reading-order spine, and navigation document. It supports `xml:lang`, document semantics, reflowable layouts, and a separate accessibility standard ([EPUB Accessibility 1.1](https://www.w3.org/TR/epub-a11y-11/)). It is stronger than Markdown for book distribution and accessibility metadata.

It is weaker as Offbeat's source/runtime model:

- authors generally edit another source format and generate EPUB;
- source review spans XML/XHTML/CSS and a ZIP container;
- metadata is publication-oriented, not a normalized substance/clinical-review schema;
- the specification defines navigation but not a mandatory portable full-text index; search is implemented by the reading system; and
- deterministic output requires canonical XML plus fixed ZIP entry order, timestamps, compression settings, and tool versions.

Flutter support exists but is reader-oriented and comparatively heavy:

- [`epubx`](https://pub.dev/packages/epubx) is a Dart EPUB reader/writer/parser exposing chapters, HTML, images, CSS, and package metadata; version 4.0.0 was published 2023-06-30. It is a parser/model, not a complete current Flutter safety-guide UI ([README](https://github.com/rbcprolabs/epubx.dart)).
- [`flutter_epub_viewer`](https://pub.dev/packages/flutter_epub_viewer) 2.0.0 (2026-07-26) combines epub.js with `flutter_inappwebview`, provides search/highlights/navigation, and requires WebView/platform setup; its Android instructions require allowing cleartext traffic for its local serving path ([README](https://github.com/fayis672/epub_viewer)).
- [`flureadium`](https://pub.dev/packages/flureadium) 0.16.6 (2026-08-08) wraps Readium for EPUB, PDF, audiobooks, comics, TTS, annotations, and more. It is actively maintained but brings native Readium setup and is LGPL-3.0 ([README](https://github.com/mulev/flureadium)).

These are reasonable choices for an ebook reader, not the smallest choice for integrated native safety cards and pages.

### 7. Static HTML5 plus a manifest

HTML is an open Living Standard with native links, images, language attributes, accessibility semantics, and broad authoring/rendering support ([WHATWG HTML](https://html.spec.whatwg.org/multipage/)). A local directory or deterministic ZIP plus a JSON manifest can be complete, portable, and easy to serve in a WebView. JSON-LD or a custom manifest can carry structured metadata.

It remains a generated format, not the best hand-authored source. Direct HTML diffs are noisier than Markdown, CSS/DOM behavior expands the security and accessibility test surface, and scripts/remote loads must be disabled or tightly controlled. [`flutter_widget_from_html_core`](https://pub.dev/packages/flutter_widget_from_html_core) can map supported HTML to Flutter widgets, while `webview_flutter` uses a system browser engine; neither supplies storage, search, content validation, or provenance.

Use self-contained HTML as an optional interchange/export target, not as the source of truth.

## Flutter/Dart package reality

Versions and publication dates below come from the primary [pub.dev package API](https://pub.dev/api/packages/flutter_markdown_plus) and linked repositories, observed 2026-08-10. A recent publication is evidence of activity, not a quality guarantee.

| Package | Observed status | What it actually does | What it does **not** do | Relevance |
|---|---|---|---|---|
| [`flutter_markdown_plus`](https://pub.dev/packages/flutter_markdown_plus) | 1.0.12, 2026-07-10; active continuation of discontinued Google package | Native Flutter Markdown widgets, GFM, links, tables, task lists, local/asset/network images, custom builders | Storage, FTS, front-matter validation, CMS, provenance | **Recommended renderer** |
| [`flutter_markdown`](https://pub.dev/packages/flutter_markdown) | 0.7.7+1, 2025-05-06; pub.dev marks discontinued and replaced by `flutter_markdown_plus` | Former Flutter Markdown renderer | Maintained future path, storage/search | Do not adopt new |
| [`markdown`](https://pub.dev/packages/markdown) | 7.3.1, 2026-03-18; Dart team repository | Markdown parser/AST-to-HTML with CommonMark/GFM-like extension sets | Flutter widgets, sanitization, storage/search | Transitive parser / useful in build tools |
| [`markdown_widget`](https://pub.dev/packages/markdown_widget) | 2.3.2+8, 2025-04-26 | Alternative native renderer with TOC, code highlighting, custom tags | Storage/search/CMS; less recent release than recommended path | Viable fallback, no present advantage |
| [`yaml`](https://pub.dev/packages/yaml) | 3.1.3, 2024-12-20; Dart team repository | YAML parsing | YAML writing, schema validation, rendering, storage | Build-time parsing only; pair with a validator |
| [`wikimedia_dart`](https://pub.dev/packages/wikimedia_dart) | 0.1.0, 2026-07-22; README says independent/unofficial | Online Wikimedia/MediaWiki REST API client | Wikitext/template renderer, XML importer, offline store | Ingestion helper at most; not needed for Bifrost GraphQL |
| [`webview_flutter`](https://pub.dev/packages/webview_flutter) | 4.14.1, 2026-07-07; Flutter-maintained | System WebView widget | Content model, authoring, search index, offline closure | Escape hatch for HTML/Tiddly/reader UIs, not preferred |
| [`epubx`](https://pub.dev/packages/epubx) | 4.0.0, 2023-06-30 | EPUB parse/read/write model | Current complete native reader UI, corpus CMS | Export/import tooling only |
| [`flutter_epub_viewer`](https://pub.dev/packages/flutter_epub_viewer) | 2.0.0, 2026-07-26 | epub.js + in-app WebView reader with reader-local search | Native clinical UI, shared app FTS/content model | Heavy but maintained EPUB option |
| [`flureadium`](https://pub.dev/packages/flureadium) | 0.16.6, 2026-08-08 | Readium-based multi-format publication reader | Small dependency/runtime surface | Best full reader option, excessive here |
| [`sqlite3`](https://pub.dev/packages/sqlite3) | 3.5.1, 2026-08-04 | Direct Dart SQLite bindings | Rendering/content management | Good generally, but duplicates Offbeat's Rust DB ownership |
| [`drift`](https://pub.dev/packages/drift) | 2.34.3, 2026-07-27 | Reactive/type-safe Dart persistence over SQLite | Rendering/authoring | Good generally, but unnecessary second database layer |

No credible maintained Dart package was found for full OpenZIM reading, TiddlyWiki WikiText rendering, or full AsciiDoc compatibility. Package-name/search false positives must not be mistaken for format support.

## Recommended smallest maintainable architecture

### Canonical repository layout

```text
safety-guide/
  schema/
    page.schema.json
    psychonautwiki-record.schema.json
  pages/
    en-GB/
      overheating.md
      serotonin-syndrome.md
    de-DE/
      overheating.md
  generated/
    psychonautwiki/
      mdma.json
      psilocybin.json
  assets/
    <sha256>.webp
```

The exact root can follow Offbeat's eventual project convention; the important properties are ownership separation and stable IDs.

### Editorial page contract

A page should have minimal, strict front matter:

```yaml
---
schemaVersion: 1
id: emergency.overheating
locale: en-GB
title: Overheating
summary: Recognise danger signs and act early.
aliases: [hyperthermia, heat stroke]
tags: [emergency, temperature]
contentKind: clinical-guide
review:
  status: clinically-reviewed
  reviewedAt: 2026-07-15
  reviewerId: clinical-board
sources:
  - sourceId: nhs.heat-exhaustion
related: [emergency.serotonin-syndrome]
generatedRefs: [psychonautwiki.mdma]
---
```

The body remains GFM prose. Do not permit inline HTML, executable content, network images, or unversioned custom Markdown extensions. Render warnings, emergency actions, dosage ranges, and interaction severity through typed application components where mistakes would be consequential.

### Generated PsychonautWiki record contract

Keep imported records separate and immutable between explicit regeneration commits. At minimum record:

- stable Offbeat entity ID and normalized source name;
- exact Bifrost query and normalized typed response;
- canonical PsychonautWiki page URL;
- source MediaWiki revision ID/timestamp where obtainable;
- retrieval timestamp and raw-response SHA-256;
- source content/license declaration;
- generator name/version and transformation/schema version;
- field-level source classification where fields are combined from multiple sources;
- validation/review status.

PsychonautWiki's API provides typed fields for routes of administration, dose ranges/units, durations, effects, interactions, toxicity, tolerance, images, and reagent results, which makes JSON a natural generated representation ([Bifrost API implementation](https://github.com/psychonautwiki/bifrost)). The API's MIT software license must not be copied onto returned wiki content. Preserve CC BY-SA attribution and share-alike obligations for imported copyrightable material, and verify image licenses separately. This is an engineering recommendation, not legal advice.

Never let regeneration overwrite clinical prose or change a reviewed clinical assertion without creating an explicit review diff/state transition.

### Deterministic compiler

A build tool should:

1. Normalize UTF-8, LF endings, Unicode form, stable IDs, locales, dates, and relative URIs.
2. Validate both schemas with unknown fields rejected.
3. Parse the selected Markdown dialect and reject raw HTML/unsafe URIs.
4. Validate all internal links, locale variants, source references, and asset hashes.
5. Join editorial pages and generated records without mutating either source.
6. Insert records in stable order into a pinned SQLite version/schema; avoid current-time/random values, use fixed PRAGMAs, and `VACUUM`/finalize consistently.
7. Build FTS5 columns for locale, title, aliases, tags, summary, and body, with title/alias weighting.
8. Emit content schema version, source commit, corpus digest, build-tool version, and license inventory.
9. Run two clean builds in CI and require identical corpus digests; if byte-for-byte SQLite reproducibility is required, also require identical database hashes under the pinned toolchain.

A deterministic *corpus digest* over canonical records is the durable invariant. SQLite byte identity can additionally be enforced in CI, but should not be assumed across SQLite/toolchain upgrades.

### Runtime

- Bundle the read-only database and local assets in the application for v1. No network is needed for core guide use.
- Let Rust open/copy/version the database and expose `getPage`, `getGeneratedRecord`, `search`, `getAsset`, and attribution models through FRB.
- Let Flutter own presentation only. Use `flutter_markdown_plus` with a strict custom link handler and image provider. Internal links resolve by stable ID; external source links are visibly external and never required to read the guide.
- Search in Rust/SQLite FTS5. Start with `unicode61` for space-delimited European languages, test diacritics and drug aliases, and select/test trigram or locale-specific tokenization for CJK and other scripts rather than assuming one tokenizer fits all.
- Store one page row per `(id, locale)`. Make locale fallback explicit and visible; do not silently mix translated headings with untranslated clinical body text.
- Show review date/status and an accessible “Sources and attribution” view per page/record.

This adds one renderer dependency and one derived content schema while reusing Offbeat's Rust/SQLite/FRB architecture. Adding Drift/sqflite, a WebView CMS, a local HTTP server, or a second search engine would increase maintenance without adding required capability.

### Safety and validation gates

Before shipping, require observable checks for:

- complete operation in airplane mode, including every image and internal link;
- no runtime remote asset fetches from guide content;
- no raw HTML/script execution or unsafe URI schemes;
- schema failures for missing review/provenance/license fields;
- broken-link and orphan-page detection;
- per-locale search tests for titles, synonyms, common misspellings, diacritics, and substance aliases;
- visible language fallback, review status, source revision, and attribution;
- generated-data changes invalidating or flagging affected clinical review rather than silently publishing;
- two-build digest reproducibility;
- database integrity and FTS integrity checks;
- screen-reader semantics, selectable text, large-text reflow, contrast, and 44 px tap targets in the Flutter renderer.

## When another format would become preferable

- Choose **MediaWiki** if Offbeat decides to run a multi-author public wiki with browser editing, permissions, talk/revision workflows, templates, and translators. Still publish a simplified compiled app format rather than embedding wikitext.
- Choose **OpenZIM** if the product must distribute very large pre-rendered websites through the Kiwix ecosystem or interchange with existing ZIM libraries/readers.
- Choose **TiddlyWiki** if the guide becomes a user-editable personal notebook and a JavaScript/WebView experience is acceptable.
- Choose **AsciiDoc** if the primary output becomes a complex technical manual with reusable includes, conditional variants, admonitions, and multiple print/web outputs, and HTML is an accepted intermediate representation.
- Choose **EPUB** if ebook-reader interoperability, pagination, annotations, TTS/media overlays, and external publication distribution outweigh native application integration.
- Generate **static HTML** when a browser-viewable/exportable corpus is needed, but keep Markdown/JSON as source.

## Primary sources

- CommonMark: [specification](https://spec.commonmark.org/), [source and conformance tests](https://github.com/commonmark/commonmark-spec), [license](https://github.com/commonmark/commonmark-spec/blob/master/LICENSE)
- GFM: [specification](https://github.github.com/gfm/), [`cmark-gfm`](https://github.com/github/cmark-gfm)
- Front matter/YAML: [Jekyll convention](https://jekyllrb.com/docs/front-matter/), [YAML 1.2.2](https://yaml.org/spec/1.2.2/)
- MediaWiki: [formatting](https://www.mediawiki.org/wiki/Help:Formatting), [export](https://www.mediawiki.org/wiki/Help:Export), [export XSD](https://www.mediawiki.org/xml/export-0.11.xsd), [ContentHandler](https://www.mediawiki.org/wiki/Manual:ContentHandler)
- OpenZIM: [file format](https://wiki.openzim.org/wiki/ZIM_file_format), [article format](https://wiki.openzim.org/wiki/Article_Format), [metadata](https://wiki.openzim.org/wiki/Metadata), [search indexes](https://wiki.openzim.org/wiki/Search_indexes), [`libzim`](https://github.com/openzim/libzim), [`zim-tools`](https://github.com/openzim/zim-tools)
- TiddlyWiki: [project/current release](https://tiddlywiki.com/), [license](https://tiddlywiki.com/static/License.html), [tiddler files](https://tiddlywiki.com/static/TiddlerFiles.html), [fields](https://tiddlywiki.com/static/TiddlerFields.html)
- AsciiDoc: [language site](https://asciidoc.org/), [Eclipse specification project](https://projects.eclipse.org/projects/asciidoc.asciidoc-lang), [Asciidoctor](https://github.com/asciidoctor/asciidoctor), [language docs](https://docs.asciidoctor.org/asciidoc/latest/)
- EPUB: [EPUB 3.3 Recommendation](https://www.w3.org/TR/epub-33/), [EPUB Accessibility 1.1](https://www.w3.org/TR/epub-a11y-11/)
- HTML: [WHATWG HTML Living Standard](https://html.spec.whatwg.org/multipage/)
- Search/storage: [SQLite FTS5](https://www.sqlite.org/fts5.html), [SQLite as an application file format](https://www.sqlite.org/appfileformat.html)
- PsychonautWiki: [Bifrost README/source](https://github.com/psychonautwiki/bifrost), [live MediaWiki rights metadata](https://psychonautwiki.org/w/api.php?action=query&meta=siteinfo&siprop=general%7Crightsinfo&format=json&formatversion=2)
- Flutter/Dart package status: [pub.dev JSON API](https://pub.dev/api/packages/flutter_markdown_plus) and each package/repository linked in the package table above
