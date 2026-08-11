import { parseArtistBilling } from "@offbeat/protocol";
import { describe, expect, it, vi } from "vitest";
import {
	ARTIST_RESOLUTION_MODEL,
	artistResolutionCacheMaterial,
	buildArtistResolutionQueries,
	createArtistResolutionAiCacheKey,
	createArtistResolutionSearchCacheKey,
	decodeArtistResolutionProposal,
	resolveArtistBilling,
	type ArtistResolutionInput,
	type TavilyArtistSearchResult,
} from "./artist-resolution";

const OPTIONS = {
	tavilyApiKey: "tavily-secret-value",
	deepSeekApiKey: "deepseek-secret-value",
	gatewayBaseUrl: "https://gateway.ai.cloudflare.com/v1/account/gateway/deepseek",
};

function inputFor(sourceBilling: string, contextBillings = ["Mya", "Other Festival Act"]): ArtistResolutionInput {
	const parsed = parseArtistBilling(sourceBilling);
	return {
		sourceBilling,
		contextBillings,
		parsedBilling: {
			coreBilling: parsed.coreBilling,
			identityHint: parsed.identityHint,
			performanceQualifiers: parsed.performanceQualifiers,
			...(parsed.presentedTitle ? { presentedTitle: parsed.presentedTitle } : {}),
		},
	};
}

function jsonResponse(value: unknown, status = 200): Response {
	return new Response(JSON.stringify(value), {
		status,
		headers: { "Content-Type": "application/json" },
	});
}

function parseRequestBody(value: BodyInit | null | undefined): Record<string, unknown> {
	try {
		return JSON.parse(String(value)) as Record<string, unknown>;
	} catch (error) {
		const detail = error instanceof Error ? error.message : String(error);
		throw new Error(`expected request body to contain JSON: ${detail}`);
	}
}

function providerFetch(results: unknown[], proposal: unknown) {
	let tavilyRequests = 0;
	return vi.fn<typeof fetch>(async (request) => {
		const url = String(request);
		if (url === "https://api.tavily.com/search") {
			const response = tavilyRequests === 0 ? { results } : { results: [] };
			tavilyRequests += 1;
			return jsonResponse(response);
		}
		if (url === `${OPTIONS.gatewayBaseUrl}/chat/completions`) {
			return jsonResponse({ choices: [{ message: { content: JSON.stringify(proposal) } }] });
		}
		throw new Error(`unexpected URL: ${url}`);
	});
}

const HARRY_RESULTS = [
	{
		title: "Midland biography",
		url: "https://example.com/midland",
		content: "Midland, also known by his real name Harry Agius, is a British DJ.",
		score: 0.99,
	},
	{
		title: "Midland interview",
		url: "https://ra.co/features/midland-harry-agius",
		content: "Midland is the project and alias of Harry Agius.",
		score: 0.98,
	},
	{
		title: "Dan Beaumont",
		url: "https://example.com/dan-beaumont",
		content: "Dan Beaumont, billed as Dan, is a DJ and promoter.",
		score: 0.98,
	},
	{
		title: "Dan Beaumont interview",
		url: "https://ra.co/features/dan-beaumont",
		content: "Dan is the moniker used by DJ Dan Beaumont for Tea Dance.",
		score: 0.97,
	},
	{
		title: "Harry & Dan Tea Dance",
		url: "https://houghtonfestival.co.uk/tea-dance",
		content: "Harry and Dan host the Tea Dance at Houghton Festival.",
		score: 0.97,
	},
];

const HARRY_PROPOSAL = {
	overallConfidence: 0.99,
	credits: [
		{
			canonicalName: "Midland",
			creditedAs: "Harry",
			role: "presenter",
			confidence: 0.99,
			evidenceIds: ["result-1", "result-2"],
		},
		{
			canonicalName: "Dan Beaumont",
			creditedAs: "Dan",
			role: "presenter",
			confidence: 0.98,
			evidenceIds: ["result-3", "result-4"],
		},
	],
	presentedTitle: "Tea Dance",
};

describe("artist billing resolution", () => {
	it("uses only bounded Tavily raw Search requests and the configured DeepSeek gateway", async () => {
		const input = inputFor("Harry & Dan Present Tea Dance");
		const fetcher = providerFetch(HARRY_RESULTS, HARRY_PROPOSAL);

		const result = await resolveArtistBilling(input, { ...OPTIONS, fetch: fetcher });

		expect(result.status).toBe("resolved");
		expect(fetcher).toHaveBeenCalledTimes(4);
		for (const [request, init] of fetcher.mock.calls.slice(0, 3)) {
			expect(request).toBe("https://api.tavily.com/search");
			expect(init).toMatchObject({
				method: "POST",
				headers: {
					Authorization: `Bearer ${OPTIONS.tavilyApiKey}`,
					"Content-Type": "application/json",
				},
			});
			const body = parseRequestBody(init?.body);
			expect(body).toMatchObject({
				search_depth: "basic",
				include_answer: false,
				include_raw_content: false,
				max_results: 5,
			});
			expect(Object.keys(body).sort()).toEqual([
				"include_answer",
				"include_raw_content",
				"max_results",
				"query",
				"search_depth",
			]);
		}
		const [gatewayRequest, gatewayInit] = fetcher.mock.calls[3];
		expect(gatewayRequest).toBe(`${OPTIONS.gatewayBaseUrl}/chat/completions`);
		expect(gatewayInit?.headers).toMatchObject({
			Authorization: `Bearer ${OPTIONS.deepSeekApiKey}`,
		});
		const gatewayBody = parseRequestBody(gatewayInit?.body);
		expect(gatewayBody.model).toBe(ARTIST_RESOLUTION_MODEL);
		expect(gatewayBody.response_format).toEqual({ type: "json_object" });
		if (!Array.isArray(gatewayBody.messages)) throw new Error("expected gateway messages");
		const [systemMessage, userMessage] = gatewayBody.messages;
		if (
			!systemMessage ||
			typeof systemMessage !== "object" ||
			!("content" in systemMessage) ||
			typeof systemMessage.content !== "string" ||
			!userMessage ||
			typeof userMessage !== "object" ||
			!("content" in userMessage) ||
			typeof userMessage.content !== "string"
		) {
			throw new Error("expected gateway message content");
		}
		expect(systemMessage.content).toContain("UNTRUSTED DATA");
		expect(systemMessage.content).toContain("Exclude collateral event guests");
		expect(userMessage.content).toContain("<untrusted_data>");
	});

	it("accepts Harry and Dan aliases while preserving the exact source and presented title", async () => {
		const sourceBilling = "Harry & Dan Present Tea Dance";
		const result = await resolveArtistBilling(inputFor(sourceBilling), {
			...OPTIONS,
			fetch: providerFetch(HARRY_RESULTS, HARRY_PROPOSAL),
		});

		expect(result).toMatchObject({
			status: "resolved",
			sourceBilling,
			presentedTitle: "Tea Dance",
			confidence: 0.99,
			credits: [
				{ canonicalName: "Midland", creditedAs: "Harry", role: "presenter" },
				{ canonicalName: "Dan Beaumont", creditedAs: "Dan", role: "presenter" },
			],
		});
		expect(result.evidence.map((item) => item.id)).toEqual([
			"result-1",
			"result-2",
			"result-3",
			"result-4",
		]);
	});

	it("links a joint billing to the same two solo artist names", async () => {
		const sourceBilling = "Dr. Banana & Josh T";
		const results = [
			{
				title: "Dr. Banana artist page",
				url: "https://example.com/dr-banana",
				content: "Dr. Banana is an electronic music artist.",
				score: 0.99,
			},
			{
				title: "Josh T artist page",
				url: "https://example.com/josh-t",
				content: "Josh T is an electronic music artist.",
				score: 0.98,
			},
		];
		const proposal = {
			overallConfidence: 0.98,
			credits: [
				{
					canonicalName: "Dr. Banana",
					creditedAs: "Dr. Banana",
					role: "performer",
					confidence: 0.99,
					evidenceIds: ["result-1"],
				},
				{
					canonicalName: "Josh T",
					creditedAs: "Josh T",
					role: "performer",
					confidence: 0.98,
					evidenceIds: ["result-2"],
				},
			],
			presentedTitle: null,
		};
		const result = await resolveArtistBilling(
			inputFor(sourceBilling, ["Dr. Banana", "Josh T", sourceBilling]),
			{ ...OPTIONS, fetch: providerFetch(results, proposal) },
		);
		expect(result).toMatchObject({
			status: "resolved",
			sourceBilling,
			credits: [
				{ canonicalName: "Dr. Banana", creditedAs: "Dr. Banana" },
				{ canonicalName: "Josh T", creditedAs: "Josh T" },
			],
		});
	});

	it("accepts the Houghton Coast 2 Coast billing without adding collateral artists", async () => {
		const sourceBilling = "COAST 2 COAST (THE GHOST & GENE ON EARTH)";
		const results = [
			{
				title: "Coast 2 Coast duo",
				url: "https://example.com/coast-2-coast",
				content: "Coast 2 Coast is The Ghost and Gene On Earth.",
				score: 1,
			},
		];
		const proposal = {
			overallConfidence: 0.97,
			credits: [
				{
					canonicalName: "The Ghost",
					creditedAs: "THE GHOST",
					role: "performer",
					confidence: 0.97,
					evidenceIds: ["result-1"],
				},
				{
					canonicalName: "Gene On Earth",
					creditedAs: "GENE ON EARTH",
					role: "performer",
					confidence: 0.96,
					evidenceIds: ["result-1"],
				},
			],
			presentedTitle: "COAST 2 COAST",
		};

		const result = await resolveArtistBilling(inputFor(sourceBilling), {
			...OPTIONS,
			fetch: providerFetch(results, proposal),
		});

		expect(result).toMatchObject({ status: "resolved", sourceBilling, presentedTitle: "COAST 2 COAST" });
		expect(result.credits.map((credit) => credit.canonicalName)).toEqual([
			"The Ghost",
			"Gene On Earth",
		]);
	});

	it("infers Tea Dance as a title in the official Houghton billing variant", async () => {
		const sourceBilling = "HARRY & DAN TEA DANCE";
		const result = await resolveArtistBilling(inputFor(sourceBilling), {
			...OPTIONS,
			fetch: providerFetch(HARRY_RESULTS, HARRY_PROPOSAL),
		});
		expect(result).toMatchObject({
			status: "resolved",
			sourceBilling,
			presentedTitle: "Tea Dance",
			credits: [
				{ canonicalName: "Midland", creditedAs: "Harry" },
				{ canonicalName: "Dan Beaumont", creditedAs: "Dan" },
			],
		});
	});

	it("reuses persisted Tavily and DeepSeek responses across shows", async () => {
		const searchCache = new Map<string, unknown>();
		const aiCache = new Map<string, unknown>();
		const cache = {
			getSearch: vi.fn(async (key: string) => searchCache.get(key) ?? null),
			putSearch: vi.fn(async (key: string, value: unknown) => {
				searchCache.set(key, value);
			}),
			getAi: vi.fn(async (key: string) => aiCache.get(key) ?? null),
			putAi: vi.fn(async (key: string, value: unknown) => {
				aiCache.set(key, value);
			}),
		};
		const input = inputFor("Harry & Dan Present Tea Dance");
		const firstFetch = providerFetch(HARRY_RESULTS, HARRY_PROPOSAL);
		const first = await resolveArtistBilling(input, { ...OPTIONS, cache, fetch: firstFetch });
		expect(first.status).toBe("resolved");
		expect(firstFetch).toHaveBeenCalledTimes(4);

		const secondFetch = vi.fn<typeof fetch>(() => {
			throw new Error("provider should not be called when cached");
		});
		const second = await resolveArtistBilling(input, { ...OPTIONS, cache, fetch: secondFetch });
		expect(second).toEqual(first);
		expect(secondFetch).not.toHaveBeenCalled();
		expect(cache.putSearch).toHaveBeenCalledTimes(1);
		expect(cache.putAi).toHaveBeenCalledTimes(1);
	});

	it.each([
		["malformed JSON", "{ definitely not JSON"],
		[
			"extra object keys",
			JSON.stringify({ ...HARRY_PROPOSAL, mbid: "fabricated-profile-fact" }),
		],
	])("returns unresolved for %s", async (_label, content) => {
		let tavilyRequests = 0;
		const fetcher = vi.fn<typeof fetch>(async (request) => {
			if (String(request) === "https://api.tavily.com/search") {
				const response = tavilyRequests === 0 ? { results: HARRY_RESULTS } : { results: [] };
				tavilyRequests += 1;
				return jsonResponse(response);
			}
			return jsonResponse({ choices: [{ message: { content } }] });
		});

		await expect(
			resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
				...OPTIONS,
				fetch: fetcher,
			}),
		).resolves.toMatchObject({ status: "unresolved", reason: "invalid_ai_output", credits: [] });
	});

	it("does not cache malformed AI output", async () => {
		const cache = {
			getSearch: vi.fn(async () => null),
			putSearch: vi.fn(),
			getAi: vi.fn(async () => null),
			putAi: vi.fn(),
		};
		const result = await resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
			...OPTIONS,
			cache,
			fetch: providerFetch(HARRY_RESULTS, { ...HARRY_PROPOSAL, invented: true }),
		});
		expect(result).toMatchObject({ status: "unresolved", reason: "invalid_ai_output" });
		expect(cache.putAi).not.toHaveBeenCalled();
	});

	it("keeps an incomplete multi-artist proposal in review", async () => {
		const proposal = { ...HARRY_PROPOSAL, credits: [HARRY_PROPOSAL.credits[0]] };
		const result = await resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
			...OPTIONS,
			fetch: providerFetch(HARRY_RESULTS, proposal),
		});
		expect(result).toMatchObject({ status: "needs_review", reason: "low_confidence" });
	});

	it("requires independent corroboration before auto-applying an alias expansion", async () => {
		const proposal = {
			overallConfidence: 0.99,
			credits: [
				{
					canonicalName: "Midland",
					creditedAs: "Harry Agius",
					role: "performer",
					confidence: 0.99,
					evidenceIds: ["result-1"],
				},
			],
			presentedTitle: null,
		};
		const result = await resolveArtistBilling(inputFor("Harry Agius"), {
			...OPTIONS,
			fetch: providerFetch([HARRY_RESULTS[0]], proposal),
		});
		expect(result).toMatchObject({ status: "needs_review", reason: "low_confidence" });
	});

	it("does not count apex and www URLs as independent alias sources", async () => {
		const proposal = {
			overallConfidence: 0.99,
			credits: [
				{
					canonicalName: "Midland",
					creditedAs: "Harry Agius",
					role: "performer",
					confidence: 0.99,
					evidenceIds: ["result-1", "result-2"],
				},
			],
			presentedTitle: null,
		};
		const results = [
			{
				title: "Midland biography",
				url: "https://example.com/midland",
				content: "Midland, also known as Harry Agius, is a British DJ.",
				score: 0.99,
			},
			{
				title: "Mirrored Midland biography",
				url: "https://www.example.com/midland-copy",
				content: "Midland is the project and alias of Harry Agius.",
				score: 0.98,
			},
		];
		const result = await resolveArtistBilling(inputFor("Harry Agius"), {
			...OPTIONS,
			fetch: providerFetch(results, proposal),
		});
		expect(result).toMatchObject({ status: "needs_review", reason: "low_confidence" });
	});

	it("treats separate publishers under a country-code registry as independent", async () => {
		const proposal = {
			overallConfidence: 0.99,
			credits: [
				{
					canonicalName: "Midland",
					creditedAs: "Harry Agius",
					role: "performer",
					confidence: 0.99,
					evidenceIds: ["result-1", "result-2"],
				},
			],
			presentedTitle: null,
		};
		const results = [
			{
				title: "Midland profile",
				url: "https://music.publisher-one.co.za/midland",
				content: "Midland, also known as Harry Agius, is a British DJ.",
				score: 0.99,
			},
			{
				title: "Midland interview",
				url: "https://artists.publisher-two.co.za/midland",
				content: "Midland is the project and alias of Harry Agius.",
				score: 0.98,
			},
		];
		const result = await resolveArtistBilling(inputFor("Harry Agius"), {
			...OPTIONS,
			fetch: providerFetch(results, proposal),
		});
		expect(result).toMatchObject({ status: "resolved", reason: "resolved" });
	});

	it("rejects an inferred title that is not present in the source billing", async () => {
		const proposal = { ...HARRY_PROPOSAL, presentedTitle: "Invented Event" };
		const result = await resolveArtistBilling(inputFor("Harry & Dan"), {
			...OPTIONS,
			fetch: providerFetch(HARRY_RESULTS, proposal),
		});
		expect(result).toMatchObject({ status: "unresolved", reason: "invalid_ai_output" });
	});

	it("rejects fabricated citations from outside the same search job", async () => {
		const proposal = structuredClone(HARRY_PROPOSAL);
		proposal.credits[0].evidenceIds = ["result-99"];
		const result = await resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
			...OPTIONS,
			fetch: providerFetch(HARRY_RESULTS, proposal),
		});

		expect(result).toMatchObject({
			status: "unresolved",
			reason: "invalid_ai_output",
			credits: [],
		});
	});

	it("rejects a collateral guest not explicitly credited by the source billing", async () => {
		const proposal = {
			overallConfidence: 0.99,
			credits: [
				...HARRY_PROPOSAL.credits,
				{
					canonicalName: "Mya",
					creditedAs: "Harry",
					role: "presenter",
					confidence: 0.99,
					evidenceIds: ["result-3"],
				},
			],
			presentedTitle: "Tea Dance",
		};
		const results = [
			...HARRY_RESULTS,
			{
				title: "Tea Dance event",
				url: "https://example.com/tea-dance",
				content: "Harry and Dan present Tea Dance with special guest Mya.",
				score: 0.97,
			},
		];
		const result = await resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
			...OPTIONS,
			fetch: providerFetch(results, proposal),
		});

		expect(result).toMatchObject({ status: "unresolved", reason: "invalid_ai_output" });
	});

	it("returns needs_review rather than auto-applying low confidence credits", async () => {
		const proposal = structuredClone(HARRY_PROPOSAL);
		proposal.overallConfidence = 0.94;
		proposal.credits[0].confidence = 0.94;
		const result = await resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
			...OPTIONS,
			fetch: providerFetch(HARRY_RESULTS, proposal),
		});

		expect(result).toMatchObject({
			status: "needs_review",
			reason: "low_confidence",
			confidence: 0.94,
		});
	});

	it("strictly decodes exact keys, bounds values, and rejects duplicate credits", () => {
		expect(decodeArtistResolutionProposal({ ...HARRY_PROPOSAL, extra: true })).toBeNull();
		expect(
			decodeArtistResolutionProposal({
				...HARRY_PROPOSAL,
				overallConfidence: Number.POSITIVE_INFINITY,
			}),
		).toBeNull();
		expect(
			decodeArtistResolutionProposal({
				...HARRY_PROPOSAL,
				credits: [{ ...HARRY_PROPOSAL.credits[0], canonicalName: "x".repeat(201) }],
			}),
		).toBeNull();
	});

	it("normalizes only HTTPS Tavily evidence and bounds snippets", async () => {
		const results = [
			...HARRY_RESULTS,
			{
				title: "Unsafe",
				url: "http://example.com/not-https",
				content: "ignored",
				score: 1,
			},
			{
				title: "Long result",
				url: "https://example.com/long",
				content: "x".repeat(5_000),
				score: 0.5,
			},
		];
		const result = await resolveArtistBilling(inputFor("Harry & Dan Present Tea Dance"), {
			...OPTIONS,
			fetch: providerFetch(results, HARRY_PROPOSAL),
		});

		expect(result.evidence.every((item) => item.url.startsWith("https://"))).toBe(true);
		expect(result.evidence.every((item) => item.content.length <= 1_500)).toBe(true);
	});

	it("builds deterministic cache hashes with all versions/settings and no secrets", async () => {
		const input = inputFor("Harry & Dan Present Tea Dance", ["Zulu", "Alpha"]);
		const equivalent = inputFor("Harry & Dan Present Tea Dance", ["Alpha", "Zulu"]);
		const evidence: TavilyArtistSearchResult[] = [
			{
				id: "result-1",
				title: "Midland biography",
				url: "https://example.com/midland",
				content: "Midland is Harry Agius.",
				score: 0.99,
			},
		];
		const [searchKey, equivalentSearchKey, aiKey, equivalentAiKey] = await Promise.all([
			createArtistResolutionSearchCacheKey(input),
			createArtistResolutionSearchCacheKey(equivalent),
			createArtistResolutionAiCacheKey(input, evidence),
			createArtistResolutionAiCacheKey(equivalent, evidence),
		]);
		const material = JSON.stringify(artistResolutionCacheMaterial(input));

		expect(searchKey).toBe(equivalentSearchKey);
		expect(aiKey).toBe(equivalentAiKey);
		expect(searchKey).toMatch(/^artist-resolution-search:[0-9a-f]{64}$/);
		expect(aiKey).toMatch(/^artist-resolution-ai:[0-9a-f]{64}$/);
		expect(material).toContain("resolverVersion");
		expect(material).toContain("promptVersion");
		expect(material).toContain("schemaVersion");
		expect(material).toContain(ARTIST_RESOLUTION_MODEL);
		expect(material).toContain("searchSettings");
		for (const secret of [OPTIONS.tavilyApiKey, OPTIONS.deepSeekApiKey]) {
			expect(searchKey).not.toContain(secret);
			expect(aiKey).not.toContain(secret);
			expect(material).not.toContain(secret);
		}
	});

	it("constructs no more than three deterministic search queries", () => {
		const input = inputFor("Harry & Dan Present Tea Dance", ["Zulu", "Alpha", "Beta"]);
		expect(buildArtistResolutionQueries(input)).toEqual(buildArtistResolutionQueries(input));
		expect(buildArtistResolutionQueries(input)).toHaveLength(3);
	});
});
