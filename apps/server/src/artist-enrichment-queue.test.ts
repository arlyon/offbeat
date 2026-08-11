import type { ArtistBillingResolution, ArtistProfile } from "@offbeat/protocol";
import { describe, expect, it, vi } from "vitest";
import type { ArtistEnrichmentMessage, ArtistEnrichmentOutcome } from "./artist-enrichment";
import {
	handleArtistEnrichmentQueue,
	type ArtistEnrichmentQueueEnv,
} from "./artist-enrichment-queue";

const messageBody: ArtistEnrichmentMessage = {
	jobId: "job-1",
	sourceKey: "mbid:a74b1b7f-71a5-4011-9441-d0b5e4122711",
	festivalId: "festival-1",
	setIds: ["set-1"],
	billing: "Example Artist",
	billingKey: "name:underworld|mbid:a74b1b7f-71a5-4011-9441-d0b5e4122711",
	contextBillings: ["Example Artist"],
	mbid: "a74b1b7f-71a5-4011-9441-d0b5e4122711",
};

const profile: ArtistProfile = {
	id: messageBody.sourceKey,
	name: "Example Artist",
	mbid: messageBody.mbid ?? "",
	aliases: [],
	genres: ["house"],
	description: "Electronic musician",
	links: [{ kind: "spotify", url: "https://open.spotify.com/artist/example" }],
	provenance: [],
	updatedAt: "2026-08-10T12:00:00.000Z",
};

const cachedResolution: ArtistBillingResolution = {
	id: "artist-resolution-v1-cached",
	sourceBilling: messageBody.billing,
	billingKey: messageBody.billingKey,
	status: "resolved",
	method: "ai",
	confidence: 0.99,
	credits: [
		{
			artistId: profile.id,
			canonicalName: profile.name,
			creditedAs: "Example Artist",
			role: "performer",
		},
	],
	performanceQualifiers: [],
	evidence: [],
	inputHash: "cached-input",
	processorVersion: "artist-resolution-v2",
	model: "deepseek-v4-flash",
	version: 1,
};

function jsonResponse(value: unknown): Response {
	return new Response(JSON.stringify(value), {
		headers: { "Content-Type": "application/json" },
	});
}

function jsonRequestBody(value: BodyInit | null | undefined): { query?: string } {
	try {
		return JSON.parse(String(value)) as { query?: string };
	} catch (error) {
		const detail = error instanceof Error ? error.message : String(error);
		throw new Error(`expected JSON request body: ${detail}`);
	}
}

function queueMessage(body: unknown) {
	return {
		id: "queue-message",
		timestamp: new Date(),
		body,
		attempts: 1,
		ack: vi.fn(),
		retry: vi.fn(),
	};
}

function queueBatch(
	...messages: Array<ReturnType<typeof queueMessage>>
): Parameters<typeof handleArtistEnrichmentQueue>[0] {
	return {
		queue: "artist-enrichment",
		messages,
		ackAll: vi.fn(),
		retryAll: vi.fn(),
	};
}

function namespace(stub: object): ArtistEnrichmentQueueEnv["MAIN_DO"] {
	return {
		idFromName: vi.fn(() => ({ toString: () => "id" })),
		get: vi.fn(() => stub),
	} as unknown as ArtistEnrichmentQueueEnv["MAIN_DO"];
}

function mainStub(overrides: Record<string, unknown> = {}) {
	const searchCache = new Map<string, unknown>();
	const providerAttempts = new Map<string, number>();
	return {
		getCachedArtistBillingResolution: vi.fn(async () => null),
		getCachedArtistEnrichment: vi.fn(async () => null),
		cacheCanonicalArtistEnrichment: vi.fn(),
		getCanonicalArtistProfiles: vi.fn(async () => []),
		getCanonicalArtistProfilesByName: vi.fn(async () => []),
		searchCanonicalArtistProfiles: vi.fn(async (query: string) => {
			const lookup = overrides.getCanonicalArtistProfilesByName;
			return typeof lookup === "function"
				? await (lookup as (names: string[]) => Promise<unknown[]>)([query])
				: [];
		}),
		getCachedArtistResolutionSearch: vi.fn(async (key: string) => searchCache.get(key) ?? null),
		putCachedArtistResolutionSearch: vi.fn(async (key: string, value: unknown) => {
			searchCache.set(key, value);
		}),
		deleteCachedArtistResolutionSearches: vi.fn(async (keys: string[]) => {
			for (const key of keys) searchCache.delete(key);
		}),
		getArtistSearchProviderAttempts: vi.fn(async (keys: string[]) =>
			Object.fromEntries(keys.map((key) => [key, providerAttempts.get(key) ?? 0])),
		),
		recordArtistSearchProviderAttempts: vi.fn(
			async (keys: string[], provider: "brave" | "tavily") => {
				const mask = provider === "brave" ? 1 : 2;
				for (const key of keys) providerAttempts.set(key, (providerAttempts.get(key) ?? 0) | mask);
			},
		),
		getCachedArtistResolutionAi: vi.fn(async () => null),
		putCachedArtistResolutionAi: vi.fn(),
		recordArtistBillingResolution: vi.fn(async (resolution) => resolution),
		applyArtistBillingResolution: vi.fn(
			async (_festivalId, resolution, profiles): Promise<unknown> => ({
				resolution,
				profiles,
				setIds: ["set-1"],
			}),
		),
		markArtistResolutionComplete: vi.fn(),
		markArtistResolutionApplied: vi.fn(),
		markArtistEnrichmentFailure: vi.fn(),
		...overrides,
	};
}

function environment(main: object, festival: object, limiter: object): ArtistEnrichmentQueueEnv {
	return {
		MAIN_DO: namespace(main),
		FESTIVAL_DO: namespace(festival),
		ARTIST_ENRICHMENT_LIMITER: namespace({
			nextArtistSearchProvider: vi.fn(async () => "tavily"),
			...limiter,
		}),
		MUSICBRAINZ_USER_AGENT: "Offbeat/Test",
	};
}

describe("artist enrichment queue", () => {
	it("prefetches five identities with one provider request", async () => {
		const names = ["Artist A", "Artist B", "Artist C", "Artist D", "Artist E"];
		const unresolved: ArtistEnrichmentOutcome = { status: "unresolved", reason: "no_unique_match" };
		const main = mainStub({ getCachedArtistEnrichment: vi.fn(async () => unresolved) });
		const festival = { applyArtistResolution: vi.fn() };
		const messages = names.map((billing, index) =>
			queueMessage({
				...messageBody,
				jobId: `batch-job-${index}`,
				billing,
				billingKey: `name:${billing.toLowerCase()}`,
				sourceKey: `name:v2:${billing.toLowerCase()}`,
				mbid: undefined,
			}),
		);
		const fetcher = vi.fn<typeof fetch>(async (_request, init) => {
			const query = jsonRequestBody(init?.body).query ?? "";
			for (const name of names) expect(query).toContain(`"${name}"`);
			return jsonResponse({
				results: names.map((name) => ({
					title: `${name} · Artist Profile`,
					url: `https://ra.co/dj/${name.toLowerCase().replace(/ /g, "")}`,
					content: name,
					score: 0.99,
				})),
			});
		});
		vi.stubGlobal("fetch", fetcher);
		const env = environment(main, festival, { reserveMusicBrainz: vi.fn() });
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(...messages), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(fetcher).toHaveBeenCalledOnce();
		expect(main.applyArtistBillingResolution).toHaveBeenCalledTimes(5);
		for (const message of messages) expect(message.ack).toHaveBeenCalledOnce();
	});

	it("retries a missed identity once with the other provider", async () => {
		const unresolved: ArtistEnrichmentOutcome = { status: "unresolved", reason: "no_unique_match" };
		const main = mainStub({ getCachedArtistEnrichment: vi.fn(async () => unresolved) });
		const message = queueMessage({
			...messageBody,
			jobId: "alternate-job",
			billing: "Missing Artist",
			billingKey: "name:missing artist",
			sourceKey: "name:v2:missing artist",
			mbid: undefined,
		});
		const fetcher = vi.fn<typeof fetch>(async (request) => {
			if (String(request).startsWith("https://api.search.brave.com/")) {
				return jsonResponse({ web: { results: [] } });
			}
			if (String(request) === "https://api.tavily.com/search") {
				return jsonResponse({ results: [] });
			}
			throw new Error(`unexpected provider request: ${String(request)}`);
		});
		vi.stubGlobal("fetch", fetcher);
		const limiter = {
			nextArtistSearchProvider: vi.fn(async () => "brave"),
			reserveMusicBrainz: vi.fn(),
		};
		const env = environment(main, {}, limiter);
		env.ARTIST_RESOLUTION_MODEL = "deepseek-v4-flash";
		env.AI_GATEWAY_BASE_URL =
			"https://gateway.ai.cloudflare.com/v1/account/gateway/deepseek";
		env.AI_GATEWAY_TOKEN = "gateway-test";
		env.DEEPSEEK_API_KEY = "deepseek-test";
		env.BRAVE_SEARCH_API_KEY = "brave-test";
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(message), env);
			await handleArtistEnrichmentQueue(queueBatch(message), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(String(fetcher.mock.calls[0]?.[0])).toContain("api.search.brave.com");
		expect(fetcher.mock.calls[1]?.[0]).toBe("https://api.tavily.com/search");
		expect(message.retry).toHaveBeenCalledOnce();
		expect(message.ack).toHaveBeenCalledOnce();
	});

	it("turns a cached profile into a deterministic signed resolution", async () => {
		const outcome: ArtistEnrichmentOutcome = { status: "enriched", profile };
		const main = mainStub({ getCachedArtistEnrichment: vi.fn(async () => outcome) });
		const festival = { applyArtistResolution: vi.fn() };
		const limiter = { reserveMusicBrainz: vi.fn() };
		const message = queueMessage(messageBody);

		await handleArtistEnrichmentQueue(queueBatch(message), environment(main, festival, limiter));

		expect(limiter.reserveMusicBrainz).not.toHaveBeenCalled();
		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({
				billingKey: messageBody.billingKey,
				method: "deterministic",
				status: "resolved",
				credits: [expect.objectContaining({ artistId: profile.id })],
			}),
			[profile],
		);
		expect(festival.applyArtistResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({ billingKey: messageBody.billingKey }),
			[profile],
			["set-1"],
		);
		expect(main.markArtistResolutionApplied).toHaveBeenCalledWith(
			"festival-1",
			messageBody.billingKey,
			expect.any(String),
			1,
		);
		expect(main.markArtistResolutionComplete).toHaveBeenCalledWith("job-1", "resolved");
		expect(message.ack).toHaveBeenCalledOnce();
		expect(message.retry).not.toHaveBeenCalled();
	});

	it("searches the canonical index before enrichment providers", async () => {
		const main = mainStub({
			searchCanonicalArtistProfiles: vi.fn(async () => [profile]),
		});
		const festival = { applyArtistResolution: vi.fn() };
		const limiter = { reserveMusicBrainz: vi.fn() };
		const message = queueMessage(messageBody);

		await handleArtistEnrichmentQueue(queueBatch(message), environment(main, festival, limiter));

		expect(main.searchCanonicalArtistProfiles).toHaveBeenCalledWith(profile.name);
		expect(main.getCachedArtistEnrichment).not.toHaveBeenCalled();
		expect(limiter.reserveMusicBrainz).not.toHaveBeenCalled();
		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({ method: "deterministic", status: "resolved" }),
			[profile],
		);
		expect(message.ack).toHaveBeenCalledOnce();
	});

	it("attaches an exact RA profile discovered through Tavily without contacting RA", async () => {
		const outcome: ArtistEnrichmentOutcome = { status: "enriched", profile };
		const main = mainStub({ getCachedArtistEnrichment: vi.fn(async () => outcome) });
		const festival = { applyArtistResolution: vi.fn() };
		const message = queueMessage(messageBody);
		const fetcher = vi.fn<typeof fetch>(async (request) => {
			expect(String(request)).toBe("https://api.tavily.com/search");
			return jsonResponse({
				results: [
					{
						title: "Example Artist · Artist Profile",
						url: "https://ra.co/dj/exampleartist",
						content: "Example Artist",
						score: 0.99,
					},
				],
			});
		});
		vi.stubGlobal("fetch", fetcher);
		const env = environment(main, festival, { reserveMusicBrainz: vi.fn() });
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(message), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(fetcher).toHaveBeenCalledOnce();
		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.any(Object),
			[
				expect.objectContaining({
					links: expect.arrayContaining([
						{ kind: "resident_advisor", url: "https://ra.co/dj/exampleartist" },
					]),
				}),
			],
		);
	});

	it("creates a provider-neutral profile from one exact RA result", async () => {
		const billing = ".VRIL (Live)";
		const unresolved: ArtistEnrichmentOutcome = { status: "unresolved", reason: "no_unique_match" };
		const main = mainStub({ getCachedArtistEnrichment: vi.fn(async () => unresolved) });
		const festival = { applyArtistResolution: vi.fn() };
		const message = queueMessage({
			...messageBody,
			jobId: "job-vril",
			billing,
			billingKey: "name:.vril (live)",
			sourceKey: "name:v2:.vril (live)",
			mbid: undefined,
		});
		const fetcher = vi.fn<typeof fetch>(async (request, init) => {
			expect(String(request)).toBe("https://api.tavily.com/search");
			const query = jsonRequestBody(init?.body).query ?? "";
			return jsonResponse({
				results: query.includes(".VRIL")
					? [
							{
								title: ".VRIL · Artist Profile",
								url: "https://ra.co/dj/vril",
								content: ".VRIL",
								score: 0.99,
							},
						]
					: [],
			});
		});
		vi.stubGlobal("fetch", fetcher);
		const env = environment(main, festival, { reserveMusicBrainz: vi.fn() });
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(message), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({
				status: "resolved",
				performanceQualifiers: ["live"],
				credits: [expect.objectContaining({ artistId: "ra:vril", creditedAs: ".VRIL" })],
			}),
			[
				expect.objectContaining({
					id: "ra:vril",
					name: ".VRIL",
					links: [{ kind: "resident_advisor", url: "https://ra.co/dj/vril" }],
				}),
			],
		);
		expect(fetcher).toHaveBeenCalledOnce();
	});

	it("falls through to web evidence when a name-only MusicBrainz lookup is unavailable", async () => {
		const main = mainStub();
		const festival = { applyArtistResolution: vi.fn() };
		const message = queueMessage({
			...messageBody,
			jobId: "job-web-fallback",
			billing: "Web Fallback Artist",
			billingKey: "name:web fallback artist",
			sourceKey: "name:v2:web fallback artist",
			mbid: undefined,
		});
		const fetcher = vi.fn<typeof fetch>(async (request) => {
			if (String(request) === "https://api.tavily.com/search") {
				return jsonResponse({
					results: [
						{
							title: "Web Fallback Artist · Artist Profile",
							url: "https://ra.co/dj/webfallbackartist",
							content: "Web Fallback Artist",
							score: 0.99,
						},
					],
				});
			}
			if (String(request).startsWith("https://musicbrainz.org/ws/2/artist")) {
				return new Response("unavailable", { status: 503 });
			}
			throw new Error(`unexpected provider request: ${String(request)}`);
		});
		vi.stubGlobal("fetch", fetcher);
		const env = environment(main, festival, { reserveMusicBrainz: vi.fn(async () => 0) });
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(message), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(message.retry).not.toHaveBeenCalled();
		expect(message.ack).toHaveBeenCalledOnce();
		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({ status: "resolved" }),
			[expect.objectContaining({ id: "ra:webfallbackartist" })],
		);
	});

	it("uses a Tavily-discovered MBID and RA link for an exact artist", async () => {
		const mbid = "ce30feb8-6664-4262-8bf3-4e30f4730fc9";
		const aokiProfile: ArtistProfile = {
			...profile,
			id: `mbid:${mbid}`,
			name: "Aoki Takamasa",
			mbid,
			links: [],
		};
		const unresolved: ArtistEnrichmentOutcome = { status: "unresolved", reason: "no_unique_match" };
		const main = mainStub({
			getCachedArtistEnrichment: vi.fn(async (sourceKey: string) =>
				sourceKey === `mbid:${mbid}`
					? { status: "enriched", profile: aokiProfile }
					: unresolved,
			),
		});
		const festival = { applyArtistResolution: vi.fn() };
		const message = queueMessage({
			...messageBody,
			jobId: "job-aoki",
			billing: "Aoki Takamasa",
			billingKey: "name:aoki takamasa",
			sourceKey: "name:v2:aoki takamasa",
			mbid: undefined,
		});
		const fetcher = vi.fn<typeof fetch>(async (_request, init) => {
			const query = jsonRequestBody(init?.body).query ?? "";
			expect(query).toContain("Aoki Takamasa");
			return jsonResponse({
				results: [
					{
						title: "Aoki Takamasa - MusicBrainz",
						url: `https://musicbrainz.org/artist/${mbid}`,
						content: "Aoki Takamasa",
						score: 0.99,
					},
					{
						title: "Aoki Takamasa · Artist Profile",
						url: "https://ra.co/dj/aokitakamasa",
						content: "Aoki Takamasa",
						score: 0.99,
					},
				],
			});
		});
		vi.stubGlobal("fetch", fetcher);
		const env = environment(main, festival, { reserveMusicBrainz: vi.fn() });
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(message), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({
				credits: [expect.objectContaining({ artistId: `mbid:${mbid}` })],
			}),
			[
				expect.objectContaining({
					mbid,
					links: [{ kind: "resident_advisor", url: "https://ra.co/dj/aokitakamasa" }],
				}),
			],
		);
	});

	it("links explicitly named talk participants as presenters", async () => {
		const participants = [
			{ ...profile, id: "artist:sonja", name: "Sonja Moonear" },
			{ ...profile, id: "artist:dr-banana", name: "Dr Banana" },
			{ ...profile, id: "artist:tristan", name: "Tristan Da Cunha" },
		];
		const main = mainStub({
			getCachedArtistEnrichment: vi.fn(async () => ({
				status: "unresolved",
				reason: "ambiguous_billing",
			})),
			getCanonicalArtistProfilesByName: vi.fn(async (names: string[]) =>
				participants.filter((candidate) => names.includes(candidate.name)),
			),
		});
		const festival = { applyArtistResolution: vi.fn() };
		const billing =
			"Talk: Digging Deep: Trevinos x Inverted Audio with Sonja Moonear, Dr Banana & Tristan Da Cunha";
		const message = queueMessage({
			...messageBody,
			jobId: "job-talk",
			billing,
			billingKey:
				"name:talk: digging deep: trevinos x inverted audio with sonja moonear, dr banana & tristan da cunha",
			sourceKey: "name:v2:talk",
			mbid: undefined,
		});

		await handleArtistEnrichmentQueue(
			queueBatch(message),
			environment(main, festival, { reserveMusicBrainz: vi.fn() }),
		);

		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({
				presentedTitle: "Digging Deep: Trevinos x Inverted Audio",
				credits: [
					expect.objectContaining({ creditedAs: "Sonja Moonear", role: "presenter" }),
					expect.objectContaining({ creditedAs: "Dr Banana", role: "presenter" }),
					expect.objectContaining({ creditedAs: "Tristan Da Cunha", role: "presenter" }),
				],
			}),
			participants,
		);
	});

	it("reuses a global resolution across shows without enrichment provider work", async () => {
		const main = mainStub({
			getCachedArtistBillingResolution: vi.fn(async () => cachedResolution),
			getCanonicalArtistProfiles: vi.fn(async () => [profile]),
		});
		const festival = { applyArtistResolution: vi.fn() };
		const limiter = { reserveMusicBrainz: vi.fn() };
		const message = queueMessage(messageBody);

		await handleArtistEnrichmentQueue(queueBatch(message), environment(main, festival, limiter));

		expect(main.getCachedArtistEnrichment).not.toHaveBeenCalled();
		expect(limiter.reserveMusicBrainz).not.toHaveBeenCalled();
		expect(festival.applyArtistResolution).toHaveBeenCalledWith(
			"festival-1",
			cachedResolution,
			[profile],
			["set-1"],
		);
		expect(main.markArtistResolutionApplied).toHaveBeenCalledWith(
			"festival-1",
			cachedResolution.billingKey,
			cachedResolution.id,
			cachedResolution.version,
		);
		expect(message.ack).toHaveBeenCalledOnce();
	});

	it("resolves a real-name alias and publishes the canonical profile", async () => {
		const aliasMessage: ArtistEnrichmentMessage = {
			...messageBody,
			sourceKey: "name:harry agius",
			billingKey: "name:harry agius",
			billing: "Harry Agius",
			contextBillings: ["Harry Agius", "Midland"],
			mbid: undefined,
		};
		const unresolved: ArtistEnrichmentOutcome = { status: "unresolved", reason: "no_unique_match" };
		const main = mainStub({
			getCachedArtistEnrichment: vi.fn(async (sourceKey: string) =>
				sourceKey === "name:v2:midland" ? { status: "enriched", profile } : unresolved,
			),
		});
		const festival = { applyArtistResolution: vi.fn() };
		let resolutionSearchCalls = 0;
		const fetcher = vi.fn<typeof fetch>(async (request, init) => {
			if (String(request) === "https://api.tavily.com/search") {
				const requestBody = jsonRequestBody(init?.body);
				const targetedDiscovery = requestBody.query?.startsWith("site:") ?? false;
				const results =
					!targetedDiscovery && resolutionSearchCalls++ === 0
						? [
								{
									title: "Midland biography",
									url: "https://example.com/midland",
									content: "Midland, also known as Harry Agius, is a British DJ.",
									score: 0.99,
								},
								{
									title: "Midland interview",
									url: "https://ra.co/features/midland",
									content: "Midland is the project and alias of Harry Agius.",
									score: 0.98,
								},
							]
						: [];
				return jsonResponse({ results });
			}
			if (
				String(request) ===
				"https://gateway.ai.cloudflare.com/v1/account/gateway/deepseek/chat/completions"
			) {
				return jsonResponse({
					choices: [
						{
							message: {
								content: JSON.stringify({
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
								}),
							},
						},
					],
				});
			}
			throw new Error(`unexpected provider request: ${String(request)}`);
		});
		vi.stubGlobal("fetch", fetcher);
		const message = queueMessage(aliasMessage);
		const env = environment(main, festival, { reserveMusicBrainz: vi.fn() });
		env.ARTIST_RESOLUTION_MODEL = "deepseek-v4-flash";
		env.AI_GATEWAY_BASE_URL =
			"https://gateway.ai.cloudflare.com/v1/account/gateway/deepseek";
		env.AI_GATEWAY_TOKEN = "gateway-test";
		env.DEEPSEEK_API_KEY = "deepseek-test";
		env.TAVILY_API_KEY = "tavily-test";

		try {
			await handleArtistEnrichmentQueue(queueBatch(message), env);
		} finally {
			vi.unstubAllGlobals();
		}

		expect(message.retry).not.toHaveBeenCalled();
		expect(main.recordArtistBillingResolution).not.toHaveBeenCalled();
		expect(main.applyArtistBillingResolution).toHaveBeenCalledWith(
			"festival-1",
			expect.objectContaining({
				status: "resolved",
				method: "ai",
				credits: [
					expect.objectContaining({ artistId: profile.id, creditedAs: "Harry Agius" }),
				],
			}),
			[profile],
		);
		expect(festival.applyArtistResolution).toHaveBeenCalledOnce();
		expect(message.ack).toHaveBeenCalledOnce();
	});

	it("keeps imports non-blocking when AI resolution is not configured", async () => {
		const unresolved: ArtistEnrichmentOutcome = { status: "unresolved", reason: "no_unique_match" };
		const main = mainStub({ getCachedArtistEnrichment: vi.fn(async () => unresolved) });
		const message = queueMessage(messageBody);

		await handleArtistEnrichmentQueue(
			queueBatch(message),
			environment(main, {}, { reserveMusicBrainz: vi.fn() }),
		);

		expect(main.recordArtistBillingResolution).toHaveBeenCalledWith(
			expect.objectContaining({ status: "unresolved", billingKey: messageBody.billingKey }),
		);
		expect(main.markArtistResolutionComplete).toHaveBeenCalledWith("job-1", "unresolved");
		expect(message.ack).toHaveBeenCalledOnce();
		expect(message.retry).not.toHaveBeenCalled();
	});

	it("acknowledges malformed internal messages without side effects", async () => {
		const main = mainStub();
		const message = queueMessage({ festivalId: "missing-fields" });

		await handleArtistEnrichmentQueue(
			queueBatch(message),
			environment(main, {}, {}),
		);

		expect(message.ack).toHaveBeenCalledOnce();
		expect(main.getCachedArtistBillingResolution).not.toHaveBeenCalled();
	});

	it("records failures and retries transient errors", async () => {
		const main = mainStub({
			getCachedArtistBillingResolution: vi.fn(async () => {
				throw new Error("temporary storage failure");
			}),
		});
		const message = queueMessage(messageBody);
		const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);

		await handleArtistEnrichmentQueue(
			queueBatch(message),
			environment(main, {}, {}),
		);

		expect(main.markArtistEnrichmentFailure).toHaveBeenCalledWith(
			messageBody.jobId,
			"temporary storage failure",
		);
		expect(consoleError).toHaveBeenCalledWith(
			"[artist-enrichment] retrying job job-1 in 30s: temporary storage failure",
		);
		consoleError.mockRestore();
		expect(message.retry).toHaveBeenCalledWith({ delaySeconds: 30 });
		expect(message.ack).not.toHaveBeenCalled();
	});
});
