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
	processorVersion: "artist-resolution-v1",
	model: "deepseek-v4-flash",
	version: 1,
};

function jsonResponse(value: unknown): Response {
	return new Response(JSON.stringify(value), {
		headers: { "Content-Type": "application/json" },
	});
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
	message: ReturnType<typeof queueMessage>,
): Parameters<typeof handleArtistEnrichmentQueue>[0] {
	return {
		queue: "artist-enrichment",
		messages: [message],
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
	return {
		getCachedArtistBillingResolution: vi.fn(async () => null),
		getCachedArtistEnrichment: vi.fn(async () => null),
		cacheCanonicalArtistEnrichment: vi.fn(),
		getCanonicalArtistProfiles: vi.fn(async () => []),
		getCachedArtistResolutionSearch: vi.fn(async () => null),
		putCachedArtistResolutionSearch: vi.fn(),
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
		ARTIST_ENRICHMENT_LIMITER: namespace(limiter),
		MUSICBRAINZ_USER_AGENT: "Offbeat/Test",
	};
}

describe("artist enrichment queue", () => {
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
				sourceKey === "name:midland" ? { status: "enriched", profile } : unresolved,
			),
		});
		const festival = { applyArtistResolution: vi.fn() };
		let tavilyCalls = 0;
		const fetcher = vi.fn<typeof fetch>(async (request) => {
			if (String(request) === "https://api.tavily.com/search") {
				const results =
					tavilyCalls++ === 0
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

		await handleArtistEnrichmentQueue(
			queueBatch(message),
			environment(main, {}, {}),
		);

		expect(main.markArtistEnrichmentFailure).toHaveBeenCalledWith(
			messageBody.jobId,
			"temporary storage failure",
		);
		expect(message.retry).toHaveBeenCalledWith({ delaySeconds: 30 });
		expect(message.ack).not.toHaveBeenCalled();
	});
});
