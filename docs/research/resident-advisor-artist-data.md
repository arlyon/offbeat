# Resident Advisor artist-profile data

_Research date: 2026-08-11. Primary RA-owned sources and bounded first-party HTTP/API observations only. This is product guidance, not legal advice._

## Recommendation

**Do not integrate RA page scraping or RA's anonymous GraphQL endpoint.** No supported public artist-lookup API was found, and RA's Website Terms require advance written authorisation for non-browser automated access. The GraphQL service used by RA's own frontend is publicly reachable but is an undocumented, unversioned internal interface—not a supported developer API.

For OFFBEAT, the compliant default is:

- keep the artist name from the festival/artist or OFFBEAT's reviewed identity record;
- optionally let an operator manually attach a reviewed canonical `https://ra.co/dj/<slug>` link;
- store and display only that name and outbound profile URL, while OFFBEAT remains non-commercial;
- do not fetch RA at runtime, bundle RA biographies/images, or import RA's “related artists” list.

If RA supplies written permission or a partner agreement, request a narrowly licensed feed containing only canonical profile names/URLs and, if explicitly authorised, bounded related-profile names/URLs. Exclude biographies, images, booking details, follower counts and editorial text.

## Findings

### No supported public artist API

No public developer documentation, registration flow, API key programme, artist schema contract, version policy, SLA, quota, caching licence or attribution rules were found in RA's public site/support material. On the research date, RA-owned `/developers`, `/developer`, `/docs`, `/api/docs`, `/openapi.json` and `/swagger.json` returned `404`; `/api` also returned `404` and is disallowed in `robots.txt`. RA's support sitemap contains artist-profile and promoter guidance, but no developer or artist-data integration documentation. [RA robots][robots] [RA support sitemap][support-sitemap]

RA does expose `POST /graphql` anonymously. A bounded schema probe showed `Query.artist(id: ID, slug: String): Artist`; `Artist` includes `id`, `name`, `contentUrl`, `biography`, `image`, and `relatedArtists(limit: Int): [Artist]`. A bounded lookup returned Four Tet as internal ID `829`, name `Four Tet`, and `contentUrl` `/dj/fourtet`. Schema introspection was enabled. This demonstrates technical accessibility only; it does not supply support, permission, or a licence. [RA GraphQL endpoint][graphql]

The current RA Next.js artist-page bundle confirms that this is RA's own frontend interface. It contains `GET_ARTIST_BY_SLUG` and a separate `GET_RELATED_ARTISTS($id: ID!)` query. The latter requests each related artist's `id`, `name`, `contentUrl`, `isFollowing`, `image`, and `followerCount`. The bundle also exposes the production setting `GRAPHQL_URL: "/graphql"`. Build-hashed frontend assets are deploy artifacts and can change without notice. [RA artist frontend bundle][artist-bundle] [RA terms page/frontend state][terms]

**Conclusion:** treat `/graphql` as an observed private/internal API. Do not make it an OFFBEAT dependency.

### “Related artists” is internal page data, not a public product

A single read-only GraphQL request, limited to ten results and to `id`, `name`, and `contentUrl`, returned ten related profiles for Four Tet. This verifies that the page component's relationship is available through internal GraphQL. No RA documentation was found defining what “related” means, how ranking is calculated, whether order is significant, how often it changes, or whether results may be retained or republished. [RA GraphQL endpoint][graphql] [RA artist frontend bundle][artist-bundle]

The relationship is therefore unsuitable as stable domain data:

- the query and fields are tied to a build-hashed frontend bundle, with no public compatibility promise;
- the schema exposes an optional limit but no documented pagination, freshness timestamp, provenance, ranking semantics or change feed;
- RA says artists can claim control of profile content and should keep profile information current, so profile data is deliberately mutable; and
- RA provides no public licence for redistributing this recommendation set. [RA artist guidance][artist-guidance]

Even a minimal related-artist import of names and profile URLs is still automated extraction and republication of RA-derived data. Reducing the fields avoids the larger copyright/personality/licensing risks of biographies and images, but does **not** cure the access and licence problem. Use it only under written RA permission that expressly covers the relationship data and OFFBEAT's offline distribution.

### Robots and technical access

RA's `robots.txt` does **not** disallow `/dj` for the generic `User-agent: *`, and RA's sitemap publishes `/dj/<slug>`, `/biography`, and `/events` URLs. It does disallow `/api` and blocks named commercial and AI crawlers site-wide. [RA robots][robots] [RA sitemap][sitemap]

That is not permission to scrape artist pages. The Terms' section 4.4(f) says the Website must not be accessed by means not authorised in writing in advance, including automated devices, scripts, bots, spiders, crawlers or scrapers, except standard search-engine technologies. Section 4.4(a) separately bars automated extraction for commercial purposes without a written agreement. The sitemap/robots allowance is consistent with ordinary search indexing, not a general data API licence. [RA terms][terms]

A bounded direct request to `/dj/fourtet` received `403` rather than profile HTML. The RA response was served through Cloudflare and carried bot-protection evidence (`Server: cloudflare`, with either a Cloudflare block page or `X-DataDome: protected`) plus `no-store`/`no-cache` directives. No challenge was solved and no bypass was attempted. This makes server-side page scraping technically brittle as well as contractually inappropriate.

### Terms and field-level rights

The UK Website Terms, accessed on the research date, state that:

- access is temporary and RA may suspend it or change/remove content without notice (sections 3.4–3.5);
- unapproved automated access is prohibited (4.4(f));
- RA and its licensors reserve all rights in the Website/content, and the stated content-use permission is for the user's own personal, non-commercial use with acknowledgement (7.1–7.2); and
- linking to RA pages is allowed only for non-commercial purposes, fairly and legally, without implying association or endorsement; RA may withdraw permission (10.1–10.2). [RA terms][terms]

RA's artist guidance says a claimed artist has control over profile content and can add a bio, booking information, musical styles, aliases, pronouns, links and a large profile photo. Those materials may therefore involve artist/uploader rights in addition to RA's rights. Public visibility is not an offline redistribution licence. [RA artist guidance][artist-guidance]

| Candidate data | Suitability for OFFBEAT |
|---|---|
| OFFBEAT/festival artist name + manually reviewed canonical RA profile URL | **Minimal acceptable use while non-commercial**, subject to RA's link terms. Keep source/review date and do not imply affiliation. |
| RA internal artist ID | Do not depend on it; undocumented and unnecessary when retaining an outbound URL. |
| RA related-artist names/profile URLs | Do not import without written permission. If licensed, cap the list, retain only name + canonical URL + retrieval time, and label it as RA-sourced. |
| Biography, blurb, editorial, booking data | Do not copy or cache; substantive protected/mutable content and no redistribution licence found. |
| Artist/cover images | Do not copy, hotlink or cache; no image licence/attribution grant was found and artist/uploader rights may apply. |
| Follower counts, relationship order, “isFollowing” | Do not use; volatile, user-specific or semantically undocumented. |

## Caching, rate limits and operations

RA publishes no rate limit or caching policy for artist GraphQL access. The bounded GraphQL responses had `CF-Cache-Status: DYNAMIC` and no observed `Cache-Control`, `Retry-After`, or standard rate-limit headers. This is **not** evidence of unlimited use. Artist-page block responses explicitly prevented caching.

Accordingly, the no-agreement design has **zero automated RA requests** and no RA response cache. Persist only operator-reviewed outbound URLs and OFFBEAT's own/festival-supplied name. Revalidate links manually during import rather than on every client or sync.

If RA authorises an integration, the written agreement should define request ceilings/concurrency, `429`/`Retry-After` handling, user-agent identification, cache TTL and purge obligations, offline redistribution, attribution, URL/ID change handling, and termination/takedown. Use server-side bounded refreshes with deduplication; never let mobile clients fan out to RA.

## Unresolved legal and product questions

1. Will RA provide written authorisation and a supported partner feed for artist lookup or related artists?
2. May OFFBEAT retain and redistribute related-profile names/URLs in an offline CRDT snapshot, and what attribution/removal duties apply?
3. Does RA permit profile links if OFFBEAT later becomes commercial, ad-supported or affiliate-funded? Section 10.1 currently says non-commercial linking.
4. What does RA's related ranking mean, how volatile is it, and may OFFBEAT describe it as a recommendation?
5. Are canonical profile redirects retained after artist renames, merges or ownership corrections?
6. Who can license each biography/image, and can RA grant the offline/mobile rights? Until answered, exclude both.

## Primary sources and bounded observations

- [RA Website Terms][terms] — sections 3.4–3.5, 4.4, 7 and 10.
- [RA robots.txt][robots] — generic `/dj` treatment, `/api` disallow, named commercial/AI crawler blocks.
- [RA sitemap index][sitemap] — generated child sitemaps include canonical artist, biography and events paths.
- [RA support sitemap][support-sitemap] — public support documentation inventory; no developer/artist-data API docs found.
- [RA artist guidance][artist-guidance] — artist control and mutable profile fields; updated 2025-06-25.
- [RA artist frontend bundle][artist-bundle] — point-in-time internal `GET_RELATED_ARTISTS` and artist queries.
- [RA GraphQL endpoint][graphql] — bounded anonymous schema and data probes described above; no authentication, bypass, bulk requests or mutation used.
- First-party HTTP observation on 2026-08-11: one profile-page request returned `403` with Cloudflare/bot-protection and no-cache headers. Requests were stopped rather than challenged or retried at scale.

[terms]: https://ra.co/terms
[robots]: https://ra.co/robots.txt
[sitemap]: https://ra.co/sitemap.xml
[support-sitemap]: https://support.ra.co/sitemap.xml
[artist-guidance]: https://support.ra.co/article/313-5-key-things-to-do-as-an-artist-on-ra
[graphql]: https://ra.co/graphql
[artist-bundle]: https://ra.co/_next/static/chunks/pages/dj/%5B...slug%5D-5dbb3844bd4ce94f.js
