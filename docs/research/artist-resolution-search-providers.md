# Artist-resolution search providers

_Research date: 2026-08-10. Primary official sources only. Pricing and policies are point-in-time; this is product guidance, not legal advice._

## Recommendation

Alternate **Brave Web Search** and **Tavily Basic Search** for server-side evidence discovery:

- batch up to five exact artist names into one query and request up to 20 results;
- alternate providers globally per uncached batch, without making both requests for the same attempt;
- map exact RA profile URLs and MusicBrainz IDs back to requested names before DeepSeek resolution;
- retain Tavily evidence in the versioned global cache, but treat Brave results as transient and delete the raw batch after processing; persist only validated identities, links, provider provenance, AI output, and billing resolutions;
- never request generated answers or raw page extraction from either provider.

**Project decision:** deterministic identifiers, canonical profiles, aliases, qualifiers, and lineup context run before paid search. One provider request covers up to five remaining identities. Brave costs **$5/1,000 requests** and includes $5 monthly credit; Tavily PAYG costs **$8/1,000 basic searches**. A 300-identity uncached import therefore needs at most about 60 search requests before retries, split across the two providers, rather than hundreds of per-identity calls. [Brave pricing][brave-pricing] [Brave API][brave-api] [Tavily pricing][tavily-pricing]

Tavily Basic remains useful because its `content` field is richer for alias evidence. Brave's title, URL, description, and optional extra snippets are sufficient for exact RA and MusicBrainz discovery, but its storage terms require transient handling of raw results. The Cloudflare Worker uses `BRAVE_SEARCH_API_KEY` and `TAVILY_API_KEY` secrets and keeps provider calls off mobile clients.

Do **not** request Tavily's optional LLM answer. `include_answer` is explicitly LLM-generated; OFFBEAT already pays DeepSeek to compare candidates and make the constrained decision. Raw Tavily results supply evidence without paying for or trusting a second answer-generation step. [Tavily Search API][tavily-search]

Use **Wikidata as a free structured corroboration layer**, especially for official websites, MusicBrainz IDs, countries and occupations. It is not a general web-search replacement. Do not use the public MusicBrainz web service commercially without arranging its commercial plan; its current official API page says only non-commercial API use is free. [Wikidata data access][wikidata-access] [Wikidata copyright][wikidata-license] [MusicBrainz API][mb-api]

## Tavily verification

### Functional and Worker fit

The official API is ordinary HTTPS JSON: `POST /search`, bearer authorization, and `Content-Type: application/json`. That works directly with the standards-based `fetch` available in a Cloudflare Worker; no Node-specific SDK or additional runtime is necessary. Its sorted `results` contain `title`, `url`, `content` (“a short description”), relevance `score`, and optional raw content. Basic/fast/advanced modes can return source chunks up to 500 characters; `max_results` defaults to five and supports up to 20. [Tavily Search API][tavily-search]

This is an API integration, not OFFBEAT scraping pages. Tavily's own OpenAPI description also calls its separate Extract product a scraping solution, so OFFBEAT should stay on **Search** and should not enable raw extraction unless a later rights review justifies it.

### Current quota and price

| Item | Verified value on 2026-08-10 |
|---|---|
| Free tier | Researcher: 1,000 API credits/month; resets monthly; no credit card |
| Basic search | 1 credit/request |
| Advanced search | 2 credits/request |
| PAYG | $0.008/credit |
| Fixed plans | Project 4,000/$30; Bootstrap 15,000/$100; Startup 38,000/$220; Growth 100,000/$500 |
| Free/development rate | 100 requests/minute |
| Production rate | 1,000 requests/minute; paid plan or PAYG required |
| Exhaustion behavior | Free requests stop until reset or upgrade; `429` responses carry `Retry-After` |

These are recurring free credits, not merely expiring trial credits. [Tavily pricing][tavily-pricing] [Tavily rate limits][tavily-rates]

### Commercial terms, attribution and retention

Tavily's Platform Terms (updated **2026-05-04**) expressly permit API integration with “Customer Applications,” including AI tools. They describe use as the customer's internal business purpose and prohibit transferring the API/key to third parties. OFFBEAT's server-side import pipeline fits that wording better than exposing Tavily directly to app users; keep the key and API calls inside the Worker. The AUP (updated **2026-05-05**) permits lawful, legitimate use and prohibits infringing use and unapproved automated extraction of Tavily itself. [Tavily Platform Terms][tavily-terms] [Tavily AUP][tavily-aup]

No mandatory “Powered by Tavily” attribution clause was found in the reviewed Platform Terms, AUP, pricing page or Search API reference. That is **not** a grant of rights in publishers' text. Preserve each source URL/title and use short snippets as evidence; do not republish full pages.

Privacy is the principal caveat:

- Tavily's Privacy Policy (updated **2025-11-24**) says it collects query data, may use portions to improve future responses, and may send queries to third-party indexes when its own index cannot answer.
- It gives no fixed deletion period for queries: personal data can be retained while the account exists, until a valid deletion request, or while needed for stated/provider purposes.
- The Platform Terms grant broad, perpetual processing rights over customer input. For generative-AI functionality, Tavily and third-party AI providers may retain inputs and outputs for model improvement.
- A standard-plan zero-data-retention commitment was not found.

Therefore query only public artist/billing facts; do not include unreleased lineup details, private contact data, credentials or sensitive personal information. `include_answer: false` avoids Tavily's explicitly generative answer feature, but does not negate the Privacy Policy's treatment of search queries. [Tavily Privacy][tavily-privacy] [Tavily Platform Terms][tavily-terms]

**Contract uncertainty:** the public terms permit Customer Application integration but do not clearly state customer ownership of Search output or an explicit long-term Search-result storage licence. OFFBEAT's proposed use—ephemeral evidence followed by storing reviewed IDs and source links—is lower risk than redistributing or warehousing Tavily payloads. Confirm in writing before persisting snippets at scale or exposing them publicly.

## Perplexity and Brave comparison

| Provider | What it is | Current price/free allowance | Rate/response | Fit and material constraint |
|---|---|---|---|---|
| **Perplexity Search API** | A **raw search backend**, not an LLM answer: ranked structured results for the caller's own processing. | $5/1,000 successful requests; one request may batch up to five queries; no token charge. No recurring free API credit was stated on the reviewed official pricing page. | 50 query units/sec; returns title, URL and snippet. | Technically strong and cheaper than Tavily PAYG. It does **not** duplicate DeepSeek reasoning. Tavily wins initially because its verified recurring free allowance covers low volume. Perplexity's published zero-retention statement is explicitly for Chat Completions, not clearly for Search. [Perplexity Search][pplx-search] [Perplexity pricing][pplx-pricing] [Perplexity rates][pplx-rates] [Perplexity privacy][pplx-privacy] |
| **Perplexity Sonar** | A grounded **LLM/chat-completions product**: it searches, synthesizes an answer, and returns citations/search results. | Sonar: $1/million input and output tokens **plus** $5/$8/$12 per 1,000 requests for low/medium/high search context. | Tier-0 Sonar: 50 requests/minute. | Using Sonar and then DeepSeek duplicates synthesis/reasoning and incurs both Sonar token/request cost and DeepSeek cost. The docs now display a deprecation/migration notice saying Sonar Chat Completions “is now Agent API”; avoid a new Sonar integration. [Perplexity Sonar][pplx-sonar] [Perplexity pricing][pplx-pricing] [Perplexity rates][pplx-rates] |
| **Brave Search API** | Raw results from Brave's own independent index; Brave explicitly says it is not a Google/Bing repackaging scraper. | $5/1,000 requests and $5 free monthly credits (about 1,000 requests). | Search plan capacity 50 queries/sec; `web.results` includes title, URL, description and optional extra snippets. | Good raw fallback and no duplicate reasoning. However, current terms (updated **2026-02-11**) generally permit only transient result storage unless a plan expressly grants storage rights, prohibit using results to train/improve AI models, and require some enriched third-party data attribution. This is awkward for durable import evidence. Provider attribution itself is optional (“may”), when used it must follow Brave's form. [Brave pricing][brave-pricing] [Brave API][brave-api] [Brave terms][brave-terms] |

If Tavily quality is inadequate, benchmark **Perplexity Search API** next—not Sonar. Brave is attractive on free cost but its storage restrictions need careful implementation.

## Other options, briefly

| Option | Verified status | Conclusion |
|---|---|---|
| **Exa Search** | $20 signup credit plus $10 monthly free credit; raw Search is $7/1,000 requests including up to ten results; default Search limit 10 QPS; results include title, URL, text/highlights. Zero Data Retention is enterprise-only. [Exa pricing][exa-pricing] [Exa Search][exa-search] [Exa rates][exa-rates] [Exa security][exa-security] | Viable and generous free allowance. More enrichment/semantic-search oriented than needed; benchmark only if Tavily misses obscure artists. Public terms broadly license Exa to retain/use inputs and outputs for service improvement. |
| **Google Custom Search JSON API / Programmable Search** | Not available to new customers. Existing customers receive 100 free queries/day, then $5/1,000 up to 10,000/day, but the service is discontinued **2027-01-01**. [Google overview][google-cse] | Reject for a new dependency despite the former attractive quota. |
| **Serper** | Official site advertises 2,500 free queries; Starter is $50 for 50,000 credits ($1/1,000), 50 QPS, credits valid six months. Its own Terms (updated **2024-05-29**) say it supplies **“web-scraped data”** and is not affiliated with Google. [Serper pricing][serper] [Serper terms][serper-terms] | Reject under the explicit “legitimate API, no scraping” requirement, regardless of price. |
| **MusicBrainz** | Excellent artist identity data, JSON, stable MBIDs, one request/sec and meaningful User-Agent. Public API use is free only for non-commercial use; commercial users are directed to commercial plans. Core database data is CC0, supplementary data CC BY-NC-SA. [MusicBrainz API][mb-api] [MusicBrainz licence][mb-license] | Keep as an identity source only under an appropriate commercial arrangement or a carefully scoped CC0 bulk-data workflow; not a free commercial web-evidence backend. |
| **Wikidata** | Main structured data is CC0. Official access guidance asks for a descriptive User-Agent, conservative concurrency and stopping on `429 Retry-After`; it publishes no simple guaranteed request quota. Results are entities/statements, not general-web snippets. [Wikidata access][wikidata-access] [Wikidata copyright][wikidata-license] | Use free as corroboration before/after web search, with caching and polite limits. One public endpoint, no API secret. |
| **Self-hosted SearXNG** | Provides a simple HTTP API with JSON/CSV/RSS when enabled. Its own limiter docs explain that upstream engines can issue CAPTCHAs or block the instance. [SearXNG API][searx-api] [SearXNG limiter][searx-limiter] | Reject for this requirement: self-hosting adds a service, operations and rate-limit uncertainty, while metasearching upstream engines does not itself grant commercial API/search-result rights. A SearXNG endpoint is fetch-compatible but not a licensed search corpus. |

## Decision and remaining uncertainty

**Decision:** batch up to five identities per request and alternate Brave Web Search with Tavily Basic Search. Use the same bounded result envelope for DeepSeek, retain Brave payloads only during queue processing, and cache Tavily payloads plus validated derived records globally. Use Wikidata as a free structured corroborator. Benchmark provider hit rates separately and set dashboard spend ceilings before enabling another production backfill.

**Re-check before production:** Tavily pricing is live web pricing rather than a fixed order form; the public contract does not specify a fixed Search-query retention period, standard-plan ZDR, unambiguous output ownership, or an explicit persistent-snippet licence. Ask Tavily to confirm those points if OFFBEAT will store snippets rather than only source links and reviewed resolution records. Perplexity did not publish a verifiable recurring free Search tier on the reviewed official page. Wikidata publishes etiquette and dynamic throttling rather than a predictable numeric SLA.

## Primary sources

- Tavily: [Search API][tavily-search], [credits/pricing][tavily-pricing], [rate limits][tavily-rates], [Platform Terms][tavily-terms], [Acceptable Use Policy][tavily-aup], [Privacy Policy][tavily-privacy]
- Perplexity: [Search quickstart][pplx-search], [Sonar quickstart/deprecation notice][pplx-sonar], [pricing][pplx-pricing], [rate limits][pplx-rates], [privacy/security][pplx-privacy]
- Brave: [Search API/pricing][brave-pricing], [Web Search docs][brave-api], [API terms][brave-terms], [privacy notice][brave-privacy]
- Alternatives: [Exa pricing][exa-pricing], [Exa Search][exa-search], [Exa rates][exa-rates], [Exa security][exa-security], [Google CSE overview][google-cse], [Serper][serper], [Serper Terms][serper-terms], [MusicBrainz API][mb-api], [MusicBrainz licence][mb-license], [Wikidata access][wikidata-access], [Wikidata copyright][wikidata-license], [SearXNG API][searx-api], [SearXNG limiter][searx-limiter]

[tavily-search]: https://docs.tavily.com/documentation/api-reference/endpoint/search
[tavily-pricing]: https://docs.tavily.com/documentation/api-credits
[tavily-rates]: https://docs.tavily.com/documentation/rate-limits
[tavily-terms]: https://www.tavily.com/terms
[tavily-aup]: https://www.tavily.com/acceptable-use-policy
[tavily-privacy]: https://www.tavily.com/privacy
[pplx-search]: https://docs.perplexity.ai/docs/search/quickstart
[pplx-sonar]: https://docs.perplexity.ai/docs/sonar/quickstart
[pplx-pricing]: https://docs.perplexity.ai/docs/getting-started/pricing
[pplx-rates]: https://docs.perplexity.ai/docs/admin/rate-limits-usage-tiers
[pplx-privacy]: https://docs.perplexity.ai/docs/resources/privacy-security
[brave-pricing]: https://brave.com/search/api/
[brave-api]: https://api-dashboard.search.brave.com/app/documentation/web-search/get-started
[brave-terms]: https://api-dashboard.search.brave.com/terms-of-service
[brave-privacy]: https://api-dashboard.search.brave.com/privacy-policy
[exa-pricing]: https://exa.ai/docs/reference/pricing
[exa-search]: https://exa.ai/docs/reference/search
[exa-rates]: https://exa.ai/docs/reference/rate-limits
[exa-security]: https://exa.ai/docs/reference/security
[google-cse]: https://developers.google.com/custom-search/v1/overview
[serper]: https://serper.dev/
[serper-terms]: https://serper.dev/terms
[mb-api]: https://musicbrainz.org/doc/MusicBrainz_API
[mb-license]: https://musicbrainz.org/doc/About/Data_License
[wikidata-access]: https://www.wikidata.org/wiki/Wikidata:Data_access
[wikidata-license]: https://www.wikidata.org/wiki/Wikidata:Copyright
[searx-api]: https://docs.searxng.org/dev/search_api.html
[searx-limiter]: https://docs.searxng.org/admin/searx.limiter.html
