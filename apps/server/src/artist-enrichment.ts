import type { ArtistLink, ArtistLinkKind, ArtistProfile } from "@offbeat/protocol";

const MUSICBRAINZ_BASE_URL = "https://musicbrainz.org/ws/2";
const WIKIDATA_ENTITY_BASE_URL = "https://www.wikidata.org/wiki/Special:EntityData";
const MAX_PROVIDER_RESPONSE_BYTES = 512 * 1024;
const MIN_SEARCH_SCORE = 95;
const MAX_ARTIST_NAME_LENGTH = 300;
const MAX_ALIAS_COUNT = 20;
const MAX_ALIAS_LENGTH = 200;
const MAX_LINK_COUNT = 20;
const MBID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const WIKIDATA_ID_PATTERN = /^Q[1-9][0-9]*$/;

export interface ArtistEnrichmentCandidate {
	festivalId: string;
	setIds: string[];
	billing: string;
	mbid?: string;
}

export interface ArtistEnrichmentMessage extends ArtistEnrichmentCandidate {
	jobId: string;
	sourceKey: string;
	billingKey: string;
	contextBillings: string[];
}

export type ArtistEnrichmentOutcome =
	| { status: "enriched"; profile: ArtistProfile }
	| { status: "unresolved"; reason: string };

interface MusicBrainzAlias {
	name?: unknown;
}

interface MusicBrainzTag {
	name?: unknown;
	count?: unknown;
}

interface MusicBrainzRelation {
	type?: unknown;
	url?: { resource?: unknown };
}

interface MusicBrainzArtist {
	id?: unknown;
	name?: unknown;
	type?: unknown;
	country?: unknown;
	area?: { name?: unknown };
	aliases?: MusicBrainzAlias[];
	tags?: MusicBrainzTag[];
	relations?: MusicBrainzRelation[];
	score?: unknown;
}

interface MusicBrainzSearchResponse {
	artists?: MusicBrainzArtist[];
}

interface WikidataEntityResponse {
	entities?: Record<
		string,
		{
			descriptions?: Record<string, { value?: unknown }>;
		}
	>;
}

export class ArtistProviderError extends Error {
	constructor(
		message: string,
		readonly retryable: boolean,
	) {
		super(message);
		this.name = "ArtistProviderError";
	}
}

export function isAmbiguousArtistBilling(value: string): boolean {
	const billing = value.normalize("NFKC").trim();
	return /(?:\s(?:b2b|vs\.?|presents?|x|feat(?:uring)?\.?|ft\.?|with)\s|\s[&+]\s|\s\/\s)/i.test(
		billing,
	);
}

export function artistEnrichmentSourceKey(billing: string, mbid?: string): string {
	if (mbid && MBID_PATTERN.test(mbid)) return `mbid:${mbid.toLowerCase()}`;
	return `name:v2:${normalizeArtistName(billing)}`;
}

export function artistEnrichmentJobId(
	festivalId: string,
	sourceKey: string,
	setIds: string[],
): string {
	const input = `v1\u0000${festivalId}\u0000${sourceKey}\u0000${[...setIds]
		.sort((left, right) => left.localeCompare(right))
		.join(",")}`;
	let hash = 2166136261;
	for (let index = 0; index < input.length; index += 1) {
		hash ^= input.charCodeAt(index);
		hash = Math.imul(hash, 16777619);
	}
	return `artist-enrichment-v1-${(hash >>> 0).toString(16).padStart(8, "0")}`;
}

export async function enrichArtist(
	candidate: ArtistEnrichmentCandidate,
	options: {
		userAgent: string;
		fetch?: typeof fetch;
		now?: () => Date;
		beforeMusicBrainzRequest?: () => Promise<void>;
		allowAmbiguousBilling?: boolean;
	},
): Promise<ArtistEnrichmentOutcome> {
	const billing = candidate.billing.normalize("NFKC").trim();
	if (!billing) return { status: "unresolved", reason: "empty_billing" };
	if (!options.allowAmbiguousBilling && isAmbiguousArtistBilling(billing)) {
		return { status: "unresolved", reason: "ambiguous_billing" };
	}

	if (candidate.mbid && !MBID_PATTERN.test(candidate.mbid)) {
		return { status: "unresolved", reason: "invalid_mbid" };
	}
	const fetcher = options.fetch ?? fetch;
	const beforeMusicBrainzRequest = options.beforeMusicBrainzRequest ?? (async () => undefined);
	const artist = candidate.mbid
		? await lookupMusicBrainzArtist(
				candidate.mbid,
				options.userAgent,
				fetcher,
				beforeMusicBrainzRequest,
			)
		: await searchMusicBrainzArtist(billing, options.userAgent, fetcher, beforeMusicBrainzRequest);
	if (!artist) return { status: "unresolved", reason: "no_unique_match" };

	const mbid = stringValue(artist.id);
	const name = stringValue(artist.name).trim();
	if (!mbid || !MBID_PATTERN.test(mbid) || !name || name.length > MAX_ARTIST_NAME_LENGTH) {
		throw new ArtistProviderError("MusicBrainz returned an invalid artist", false);
	}
	if (candidate.mbid) {
		const verifiedNames = [
			name,
			...(Array.isArray(artist.aliases)
				? artist.aliases.map((alias) => stringValue(alias.name))
				: []),
		];
		if (
			!verifiedNames.some((value) => normalizeArtistName(value) === normalizeArtistName(billing))
		) {
			return { status: "unresolved", reason: "mbid_name_mismatch" };
		}
	}

	const retrievedAt = (options.now ?? (() => new Date()))().toISOString();
	const relations = Array.isArray(artist.relations) ? artist.relations : [];
	const links = collectLinks(relations);
	const wikidataId = findWikidataId(relations);
	const wikidata = wikidataId
		? await fetchWikidataEntity(wikidataId, options.userAgent, fetcher)
		: null;
	const description = wikidataDescription(wikidata, wikidataId);
	const aliases = uniqueStrings(
		(Array.isArray(artist.aliases) ? artist.aliases : []).map((alias) => stringValue(alias.name)),
	)
		.filter(
			(alias) =>
				alias.length <= MAX_ALIAS_LENGTH &&
				normalizeArtistName(alias) !== normalizeArtistName(name),
		)
		.slice(0, MAX_ALIAS_COUNT);
	const genres = collectGenres(artist.tags);
	const musicBrainzUrl = `https://musicbrainz.org/artist/${mbid}`;
	const provenance: ArtistProfile["provenance"] = [
		{
			field: "identity,aliases,type,country",
			provider: "musicbrainz",
			sourceUrl: musicBrainzUrl,
			license: "CC0",
			retrievedAt,
		},
	];
	if (genres.length > 0) {
		provenance.push({
			field: "genres",
			provider: "musicbrainz",
			sourceUrl: musicBrainzUrl,
			license: "CC BY-SA",
			retrievedAt,
		});
	}
	if (links.length > 0) {
		provenance.push({
			field: "links",
			provider: "musicbrainz",
			sourceUrl: musicBrainzUrl,
			license: "CC0",
			retrievedAt,
		});
	}
	if (description && wikidataId) {
		provenance.push({
			field: "description",
			provider: "wikidata",
			sourceUrl: `https://www.wikidata.org/wiki/${wikidataId}`,
			license: "CC0",
			retrievedAt,
		});
	}

	return {
		status: "enriched",
		profile: {
			id: `mbid:${mbid.toLowerCase()}`,
			name,
			mbid: mbid.toLowerCase(),
			...(wikidataId ? { wikidataId } : {}),
			aliases,
			...(limitedString(artist.type, 80) ? { artistType: limitedString(artist.type, 80) } : {}),
			...(limitedString(artist.country, 80) || limitedString(artist.area?.name, 120)
				? { country: limitedString(artist.country, 80) || limitedString(artist.area?.name, 120) }
				: {}),
			genres,
			...(description ? { description } : {}),
			links,
			provenance,
			updatedAt: retrievedAt,
		},
	};
}

async function searchMusicBrainzArtist(
	billing: string,
	userAgent: string,
	fetcher: typeof fetch,
	beforeRequest: () => Promise<void>,
): Promise<MusicBrainzArtist | null> {
	const query = providerUrl(`${MUSICBRAINZ_BASE_URL}/artist`);
	const escapedBilling = billing.replaceAll("\\", "\\\\").replaceAll('"', '\\"');
	query.searchParams.set("query", `artist:"${escapedBilling}"`);
	query.searchParams.set("fmt", "json");
	query.searchParams.set("limit", "5");
	await beforeRequest();
	const response = await fetchProviderJson<MusicBrainzSearchResponse>(query, userAgent, fetcher);
	const exactMatches = (Array.isArray(response.artists) ? response.artists : []).filter(
		(artist) => {
			const score = numberValue(artist.score);
			if (score < MIN_SEARCH_SCORE) return false;
			const names = [
				stringValue(artist.name),
				...(Array.isArray(artist.aliases)
					? artist.aliases.map((alias) => stringValue(alias.name))
					: []),
			];
			return names.some((name) => normalizeArtistName(name) === normalizeArtistName(billing));
		},
	);
	if (exactMatches.length !== 1) return null;
	const mbid = stringValue(exactMatches[0].id);
	return mbid ? lookupMusicBrainzArtist(mbid, userAgent, fetcher, beforeRequest) : null;
}

async function lookupMusicBrainzArtist(
	mbid: string,
	userAgent: string,
	fetcher: typeof fetch,
	beforeRequest: () => Promise<void>,
): Promise<MusicBrainzArtist> {
	if (!MBID_PATTERN.test(mbid)) {
		throw new ArtistProviderError("Invalid MusicBrainz artist ID", false);
	}
	const url = providerUrl(`${MUSICBRAINZ_BASE_URL}/artist/${mbid.toLowerCase()}`);
	url.searchParams.set("fmt", "json");
	url.searchParams.set("inc", "aliases+tags+url-rels");
	await beforeRequest();
	return fetchProviderJson<MusicBrainzArtist>(url, userAgent, fetcher);
}

async function fetchWikidataEntity(
	wikidataId: string,
	userAgent: string,
	fetcher: typeof fetch,
): Promise<WikidataEntityResponse | null> {
	try {
		return await fetchProviderJson<WikidataEntityResponse>(
			providerUrl(`${WIKIDATA_ENTITY_BASE_URL}/${wikidataId}.json`),
			userAgent,
			fetcher,
		);
	} catch (error) {
		if (error instanceof ArtistProviderError && !error.retryable) return null;
		throw error;
	}
}

async function readBoundedProviderText(response: Response): Promise<string> {
	if (!response.body) return "";
	const reader = response.body.getReader();
	const chunks: Uint8Array[] = [];
	let total = 0;
	while (true) {
		const { done, value } = await reader.read();
		if (done) break;
		total += value.byteLength;
		if (total > MAX_PROVIDER_RESPONSE_BYTES) {
			await reader.cancel();
			throw new ArtistProviderError("Provider response is too large", false);
		}
		chunks.push(value);
	}
	const bytes = new Uint8Array(total);
	let offset = 0;
	for (const chunk of chunks) {
		bytes.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return new TextDecoder().decode(bytes);
}

async function fetchProviderJson<T>(
	url: URL,
	userAgent: string,
	fetcher: typeof fetch,
): Promise<T> {
	const controller = new AbortController();
	const timeout = setTimeout(() => controller.abort(), 10_000);
	try {
		const response = await fetcher(url, {
			headers: { Accept: "application/json", "User-Agent": userAgent },
			signal: controller.signal,
		});
		if (!response.ok) {
			throw new ArtistProviderError(
				`Provider request failed with ${response.status}`,
				response.status === 429 || response.status >= 500,
			);
		}
		const contentLength = Number(response.headers.get("content-length"));
		if (Number.isFinite(contentLength) && contentLength > MAX_PROVIDER_RESPONSE_BYTES) {
			throw new ArtistProviderError("Provider response is too large", false);
		}
		const text = await readBoundedProviderText(response);
		try {
			return JSON.parse(text) as T;
		} catch (error) {
			throw new ArtistProviderError(`Provider returned invalid JSON: ${String(error)}`, false);
		}
	} catch (error) {
		if (error instanceof ArtistProviderError) throw error;
		throw new ArtistProviderError(`Provider request failed: ${String(error)}`, true);
	} finally {
		clearTimeout(timeout);
	}
}

function collectGenres(tags: MusicBrainzTag[] | undefined): string[] {
	return (Array.isArray(tags) ? tags : [])
		.map((tag) => ({ name: stringValue(tag.name).trim(), count: numberValue(tag.count) }))
		.filter((tag) => tag.name.length > 0)
		.sort((left, right) => right.count - left.count || left.name.localeCompare(right.name))
		.filter((tag) => tag.name.length <= 80)
		.map((tag) => tag.name.toLowerCase())
		.filter((tag, index, values) => values.indexOf(tag) === index)
		.slice(0, 5);
}

function collectLinks(relations: MusicBrainzRelation[]): ArtistLink[] {
	const links = new Map<string, ArtistLink>();
	for (const relation of relations) {
		const rawUrl = stringValue(relation.url?.resource);
		const url = safeExternalUrl(rawUrl);
		if (!url || findWikidataId([relation])) continue;
		const kind = classifyLink(url, stringValue(relation.type));
		links.set(`${kind}:${url}`, { kind, url });
	}
	return [...links.values()]
		.sort((left, right) => left.kind.localeCompare(right.kind) || left.url.localeCompare(right.url))
		.slice(0, MAX_LINK_COUNT);
}

function classifyLink(url: string, relationType: string): ArtistLinkKind {
	let parsed: URL;
	try {
		parsed = new URL(url);
	} catch {
		return "other";
	}
	const hostname = parsed.hostname.toLowerCase().replace(/^www\./, "");
	if (hostname === "open.spotify.com") return "spotify";
	if (hostname === "soundcloud.com") return "soundcloud";
	if (hostname === "ra.co" && /^\/dj\/[^/]+\/?$/.test(parsed.pathname)) {
		return "resident_advisor";
	}
	if (hostname === "youtube.com" || hostname === "youtu.be") return "youtube";
	if (hostname === "instagram.com") return "instagram";
	if (hostname === "facebook.com") return "facebook";
	if (hostname === "x.com" || hostname === "twitter.com") return "x";
	if (/official homepage/i.test(relationType)) return "website";
	return "other";
}

function findWikidataId(relations: MusicBrainzRelation[]): string | undefined {
	for (const relation of relations) {
		const resource = stringValue(relation.url?.resource);
		if (!resource) continue;
		try {
			const url = new URL(resource);
			if (url.hostname !== "www.wikidata.org" && url.hostname !== "wikidata.org") continue;
			const id = url.pathname.split("/").filter(Boolean).at(-1);
			if (id && WIKIDATA_ID_PATTERN.test(id)) return id;
		} catch {
			// Ignore malformed provider relationships.
		}
	}
	return undefined;
}

function wikidataDescription(
	response: WikidataEntityResponse | null,
	wikidataId: string | undefined,
): string | undefined {
	if (!response || !wikidataId) return undefined;
	const description = stringValue(response.entities?.[wikidataId]?.descriptions?.en?.value).trim();
	return description && description.length <= 280 ? description : undefined;
}

function providerUrl(value: string): URL {
	try {
		return new URL(value);
	} catch {
		throw new ArtistProviderError(`Invalid provider URL: ${value}`, false);
	}
}

function safeExternalUrl(rawUrl: string): string | null {
	if (!rawUrl || rawUrl.length > 2048) return null;
	try {
		const url = new URL(rawUrl);
		if (url.protocol !== "https:" || url.username || url.password) return null;
		url.hash = "";
		return url.toString();
	} catch {
		return null;
	}
}

function normalizeArtistName(value: string): string {
	return value
		.normalize("NFKC")
		.toLocaleLowerCase()
		.replace(/[^\p{Letter}\p{Number}]+/gu, " ")
		.trim();
}

function uniqueStrings(values: string[]): string[] {
	return [...new Set(values.map((value) => value.trim()).filter(Boolean))].sort((left, right) =>
		left.localeCompare(right),
	);
}

function stringValue(value: unknown): string {
	return typeof value === "string" ? value : "";
}

function limitedString(value: unknown, maxLength: number): string {
	const text = stringValue(value).trim();
	return text.length <= maxLength ? text : "";
}

function numberValue(value: unknown): number {
	if (typeof value === "number" && Number.isFinite(value)) return value;
	if (typeof value === "string") {
		const parsed = Number(value);
		return Number.isFinite(parsed) ? parsed : 0;
	}
	return 0;
}
