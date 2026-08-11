# Artist enrichment benchmark

_Research date: 2026-08-10. Primary sources only. This is product guidance, not legal advice._

## Executive recommendation

Offbeat should make the offline artist surface **schedule-first, not a miniature streaming service**.

- **Minimum scope:** artist name; festival set(s), stage and time; local favourite/reminder; stable internal artist ID plus reviewed MusicBrainz/Wikidata IDs where available; a short festival-supplied or CC0 description; a small genre/tag set; canonical external links (Spotify, SoundCloud, website/social) shown as outbound actions; and one offline image only when the festival supplied it with redistribution rights or a Wikimedia Commons file has compatible per-file rights and complete attribution.
- **Extended scope:** a longer festival-authored biography, Commons photo, related _lineup_ artists, and online-only Spotify/SoundCloud/YouTube actions. Keep provider payloads out of the festival's permanent offline CRDT snapshot unless their terms expressly permit that use.
- **Do not ship by default:** cached music/video, cached Spotify or SoundCloud artist imagery/biographies, provider-derived top tracks, or Spotify-derived related artists. These add revocation, refresh, attribution, commercial-use, and offline-rights obligations disproportionate to their value.

## What comparable products actually emphasize

The consistent baseline is planning, not encyclopedic artist data:

| Capability | Evidence in comparable official surfaces | Benchmark conclusion |
|---|---|---|
| Set time/stage and personal schedule | Woov advertises lineup and personal schedule; Appic advertises lineup/timetable and favourite-artist notifications; Reading and Download advertise custom schedules/reminders; Tomorrowland advertises a live timetable and favourites. [Woov][woov] [Appic][appic] [Reading][reading] [Download][download] [Tomorrowland][tomorrowland] | **Universal/core.** |
| Favourite/follow/reminder | Coachella supports favourite artists; Download has set-time reminders; Appic's official screenshots advertise favourite-artist notifications; Roskilde artist pages expose “Follow the artist.” [Coachella][coachella] [Download][download] [Appic][appic] [Roskilde artist][roskilde-artist] | **Very common/core.** |
| Spotify/playlists | Coachella advertises curated playlists; Roskilde advertises playlists and its artist pages link to the corresponding Spotify artist; Glastonbury's official screenshots advertise Spotify-based lineup recommendations; Tomorrowland promises discovery of “new music.” [Coachella][coachella] [Roskilde app][roskilde-app] [Roskilde artist][roskilde-artist] [Glastonbury][glastonbury] [Tomorrowland][tomorrowland] | **Common as an online link/playlist/recommendation layer, not evidence of offline playback.** |
| Description/biography and photo | Roskilde's official artist surface contains a hero photo, country, editorial strapline, long biography and show context. [Roskilde artist][roskilde-artist] | **Valuable, but not demonstrated as universal among planner apps.** Festival-authored editorial is the strongest source. |
| Related/recommended artists | Roskilde shows “If you like this …” lineup artists; Appic advertises personalised _event_ recommendations; Glastonbury advertises lineup recommendations based on Spotify activity. [Roskilde artist][roskilde-artist] [Appic][appic] [Glastonbury][glastonbury] | **Common at discovery level; artist-to-artist recommendations are extended scope.** Prefer relationships among artists already on the lineup. |
| Genres/tags | Roskilde biographies describe genres and some recommendation chips include descriptors such as “High Energy,” but the reviewed app listings do not establish a standard structured genre block. [Roskilde artist][roskilde-artist] | **Useful compact metadata, not a must-have benchmark feature.** |
| Social links | Coachella exposes an app-level Socials destination; the reviewed primary evidence does not establish artist-level social buttons as common. [Coachella][coachella] | **Optional outbound links.** Do not promise availability or offline content. |
| Top tracks | Spotify exposes a top-tracks endpoint, but its current documentation marks it deprecated; no reviewed comparable listing makes artist top tracks a core promise. [Spotify top tracks][spotify-top] | **Avoid as a product dependency.** |
| Videos/playback | Coachella links to its YouTube livestream and Spotify playlist; Roskilde artist pages contain consent-gated embedded marketing content, but this is online content rather than an offline asset. [Coachella][coachella] [Roskilde artist][roskilde-artist] | **Online-only extension.** |

Woov and Appic are current, genuinely comparable planners, but their first-party listings describe planning/social features rather than detailed artist metadata. FestPilot's official domain currently redirects to a parked sale page, so no current product claims were inferred from it.

## Rights, API, caching and offline constraints

### MusicBrainz

MusicBrainz is the best **identity/disambiguation spine**: artist search supports aliases, area/country, type, life span, ISNI, tags and a specific disambiguation comment; lookup responses provide a stable MBID. [MusicBrainz search][mb-search] The API requires a meaningful User-Agent and no more than one request per second, and its public web service is free for non-commercial use while commercial users are directed to commercial plans. [MusicBrainz API][mb-api] [MusicBrainz rate limits][mb-rate]

MusicBrainz distinguishes CC0 core data from supplementary data under CC BY-SA; consequently, preserve source/license provenance by field rather than treating every returned tag, annotation or rating identically. [MusicBrainz data licence][mb-license] Local precomputation is practical, but a bulk or commercial service should use the appropriate feed/plan rather than fan out mobile requests.

### Wikidata, Wikipedia and images

Wikidata's structured main/property/lexeme data is CC0, making IDs, short descriptions, external IDs and factual statements comparatively safe to package offline. [Wikidata licensing][wikidata-license] Wikipedia article text is generally CC BY-SA 4.0/GFDL: offline extracts are possible, but require attribution and share-alike compliance; they are not “free of obligations.” [Wikipedia copyrights][wikipedia-copyright]

Wikimedia Commons is suitable only with a **per-file licence check**. Commons states that files can have different attribution/licence/share-alike requirements, provides no warranty of copyright status, and warns about separate personality, moral and privacy rights. Store creator, source page, licence/version and required credit with every cached image. [Commons reuse][commons-reuse] Festival/artist press images must not be copied merely because they are publicly reachable; obtain explicit redistribution/offline rights.

### Spotify

Spotify artist responses can contain URLs and images, while genres and popularity are currently marked deprecated. Spotify requires its logo attribution and a link back when displaying Spotify metadata, cover art or artist images, and forbids downloading Spotify content. [Spotify artist API][spotify-artist] Its Developer Terms prohibit indefinite storage and permit only temporary local caching of metadata/cover art as strictly necessary; offline sound-recording downloads are conditional and Premium-only. [Spotify terms][spotify-terms] Platform streaming is Premium-only except permitted widgets/audio previews, and previews have promotion/link-back constraints. [Spotify policy][spotify-policy]

Therefore:

- a plain, reviewed `open.spotify.com/artist/...` outbound link is preferable to bundling API-derived Spotify metadata;
- Spotify images, biographies, previews and track data should **not** enter Offbeat's durable offline festival snapshot;
- top-tracks and related-artists APIs are currently deprecated, making both poor foundations even before licensing concerns. [Spotify top tracks][spotify-top] [Spotify related artists][spotify-related]

### SoundCloud

SoundCloud permits API playback via its widget or streams, subject to track restrictions and attribution to both uploader and SoundCloud with a backlink. [SoundCloud guide][soundcloud-guide] Its API terms say User Content remains controlled by uploaders, prohibit persistent caching/file-save functionality, permit only session caching, and expressly prohibit offline access (including temporary offline listening). They also narrowly define acceptable commercial uses and allow case-by-case approval. [SoundCloud terms][soundcloud-terms]

Treat SoundCloud as an **online outbound link or compliant online widget**, never an offline enrichment source for audio, artwork, descriptions or profiles without separate rights.

### YouTube/video

YouTube forbids downloading, importing, backing up, caching or storing audiovisual content and forbids offline playback without prior written approval. Non-authorized API metadata may be held for at most 30 days before deletion or refresh. [YouTube policies][youtube-policy] Use a normal online embed/deep link; never package thumbnails/video as permanent festival data unless separately licensed outside the API.

## Recommended Offbeat scope

### Minimum, safe offline surface

1. **Festival facts:** lineup display name, all sets/stages/times, cancellation/change state.
2. **Local planning:** favourite/star, reminder, conflict context and notes. These are Offbeat/user data, not provider content.
3. **Resolved identity:** internal ID, resolution status, MusicBrainz MBID and Wikidata QID where reviewed; aliases, type and country only when useful.
4. **Compact editorial:** one festival-provided sentence, or a Wikidata CC0 short description with provenance. Do not synthesize or copy Wikipedia prose silently.
5. **Genres:** up to 3–5 normalized tags from an approved source, with source/licence recorded and an “unknown” state.
6. **Links:** canonical website plus available Spotify, SoundCloud and social URLs. Render as “Open in …”; gracefully disable while offline.
7. **Image:** festival-supplied/licensed asset or vetted Commons asset, bundled with machine-readable attribution. Otherwise use a deterministic placeholder.

### Extended, opt-in enrichment

- Festival-authored long biography and properly licensed press photo.
- Related artists **restricted to the current lineup**, computed from explicit editorial links or Offbeat-owned genre overlap; label why they are related.
- Online Spotify/SoundCloud/YouTube links or compliant embeds, with provider attribution and availability checks.
- Wikipedia extract only after implementing CC BY-SA attribution, source revision tracking and share-alike handling.
- Optional connected-time refresh records (`provider`, `providerId`, `retrievedAt`, `expiresAt`, `licence`, `attribution`, `sourceUrl`); keep these separate from durable offline-safe fields.

Do not make completeness a requirement: emerging/local acts are exactly where databases are sparse and name collisions are most dangerous.

## Separate identity resolution from enrichment

### Import-time artist disambiguation (blocking data-quality step)

This step answers **“which artist is this?”**, not “what can we show about them?”

1. Normalize the lineup credit without discarding the original display string.
2. Search MusicBrainz by name/alias; compare candidate disambiguation, type, country/area, active dates and known official/provider URLs. [MusicBrainz search][mb-search]
3. Use festival-provided website/Spotify/SoundCloud IDs as corroborating evidence, not name similarity alone.
4. Auto-resolve only a uniquely high-confidence candidate; otherwise require operator selection or mark unresolved. Persist candidate/evidence/audit state.
5. Never block lineup import on biography, photo, genres, recommendations or streaming-provider availability.

### Post-identification background enrichment (non-blocking)

After identity is stable, a rate-limited server job can resolve Wikidata/external links, normalize genres, select licensed imagery and attach editorial text. Every field should carry provenance, licence/terms class, retrieval time and an offline-redistribution flag. Failures leave a valid sparse profile. Provider-restricted data stays refreshable/online-only and must not be gossiped as permanent offline content.

## Sources consulted

All links below are first-party product listings/pages or provider documentation/policies.

### Comparable apps and festival surfaces

- [Woov — App Store listing][woov]
- [Appic — App Store listing][appic]
- [Coachella Official — App Store listing][coachella]
- [Official Glastonbury App 2025 — App Store listing][glastonbury]
- [Roskilde Festival — App Store listing][roskilde-app]
- [Roskilde official Gorillaz artist page][roskilde-artist]
- [Tomorrowland Belgium — App Store listing][tomorrowland]
- [Reading Festival — App Store listing][reading]
- [Download Festival — App Store listing][download]
- FestPilot official domain (`https://festpilot.com/`; parked/redirected, no product claims used)

### Data, media and platform policies

- [MusicBrainz API][mb-api], [search documentation][mb-search], [rate limiting][mb-rate], and [data licence][mb-license]
- [Wikidata licensing][wikidata-license]
- [Wikipedia copyrights/reuse][wikipedia-copyright]
- [Wikimedia Commons reuse guidance][commons-reuse]
- [Spotify Developer Terms][spotify-terms], [Developer Policy][spotify-policy], [artist API][spotify-artist], [top-tracks API][spotify-top], and [related-artists API][spotify-related]
- [SoundCloud API guide][soundcloud-guide] and [API Terms][soundcloud-terms]
- [YouTube API Developer Policies][youtube-policy]

[woov]: https://apps.apple.com/gb/app/woov-your-festival-companion/id1580044680
[appic]: https://apps.apple.com/gb/app/appic-festivals-more/id968362389
[coachella]: https://apps.apple.com/gb/app/coachella-official/id632833729
[glastonbury]: https://apps.apple.com/gb/app/official-glastonbury-app-2025/id6502346488
[roskilde-app]: https://apps.apple.com/gb/app/roskilde-festival/id514909274
[roskilde-artist]: https://www.roskilde-festival.dk/en/line-up/music/gorillaz
[tomorrowland]: https://apps.apple.com/gb/app/tomorrowland-belgium/id652140946
[reading]: https://apps.apple.com/gb/app/reading-festival/id1024718630
[download]: https://apps.apple.com/gb/app/download-festival/id439568471
[mb-api]: https://musicbrainz.org/doc/MusicBrainz_API
[mb-search]: https://musicbrainz.org/doc/MusicBrainz_API/Search
[mb-rate]: https://musicbrainz.org/doc/MusicBrainz_API/Rate_Limiting
[mb-license]: https://musicbrainz.org/doc/About/Data_License
[wikidata-license]: https://www.wikidata.org/wiki/Wikidata:Licensing
[wikipedia-copyright]: https://en.wikipedia.org/wiki/Wikipedia:Copyrights
[commons-reuse]: https://commons.wikimedia.org/wiki/Commons:Reusing_content_outside_Wikimedia
[spotify-terms]: https://developer.spotify.com/terms
[spotify-policy]: https://developer.spotify.com/policy
[spotify-artist]: https://developer.spotify.com/documentation/web-api/reference/get-an-artist
[spotify-top]: https://developer.spotify.com/documentation/web-api/reference/get-an-artists-top-tracks
[spotify-related]: https://developer.spotify.com/documentation/web-api/reference/get-an-artists-related-artists
[soundcloud-guide]: https://developers.soundcloud.com/docs/api/guide
[soundcloud-terms]: https://developers.soundcloud.com/docs/api/terms-of-use
[youtube-policy]: https://developers.google.com/youtube/terms/developer-policies
