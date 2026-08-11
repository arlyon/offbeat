import type { ArtistProfile } from "@offbeat/protocol";
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

describe("artist enrichment queue", () => {
	it("publishes a cached profile without another provider reservation", async () => {
		const outcome: ArtistEnrichmentOutcome = { status: "enriched", profile };
		const main = {
			getCachedArtistEnrichment: vi.fn(async () => outcome),
			applyArtistEnrichment: vi.fn(async () => ({ profile, setIds: ["set-1"] })),
			markArtistEnrichmentFailure: vi.fn(),
		};
		const festival = { applyArtistEnrichment: vi.fn() };
		const limiter = { reserveMusicBrainz: vi.fn() };
		const env: ArtistEnrichmentQueueEnv = {
			MAIN_DO: namespace(main),
			FESTIVAL_DO: namespace(festival),
			ARTIST_ENRICHMENT_LIMITER: namespace(limiter),
			MUSICBRAINZ_USER_AGENT: "Offbeat/Test",
		};
		const message = queueMessage(messageBody);

		await handleArtistEnrichmentQueue(queueBatch(message), env);

		expect(limiter.reserveMusicBrainz).not.toHaveBeenCalled();
		expect(main.applyArtistEnrichment).toHaveBeenCalledWith(messageBody, outcome);
		expect(festival.applyArtistEnrichment).toHaveBeenCalledWith(
			"festival-1",
			profile,
			["set-1"],
		);
		expect(message.ack).toHaveBeenCalledOnce();
		expect(message.retry).not.toHaveBeenCalled();
	});

	it("acknowledges malformed internal messages without side effects", async () => {
		const main = {
			getCachedArtistEnrichment: vi.fn(),
			applyArtistEnrichment: vi.fn(),
			markArtistEnrichmentFailure: vi.fn(),
		};
		const env: ArtistEnrichmentQueueEnv = {
			MAIN_DO: namespace(main),
			FESTIVAL_DO: namespace({}),
			ARTIST_ENRICHMENT_LIMITER: namespace({}),
			MUSICBRAINZ_USER_AGENT: "Offbeat/Test",
		};
		const message = queueMessage({ festivalId: "missing-fields" });

		await handleArtistEnrichmentQueue(queueBatch(message), env);

		expect(message.ack).toHaveBeenCalledOnce();
		expect(main.getCachedArtistEnrichment).not.toHaveBeenCalled();
	});

	it("records failures and retries transient errors", async () => {
		const main = {
			getCachedArtistEnrichment: vi.fn(async () => {
				throw new Error("temporary storage failure");
			}),
			applyArtistEnrichment: vi.fn(),
			markArtistEnrichmentFailure: vi.fn(),
		};
		const env: ArtistEnrichmentQueueEnv = {
			MAIN_DO: namespace(main),
			FESTIVAL_DO: namespace({}),
			ARTIST_ENRICHMENT_LIMITER: namespace({}),
			MUSICBRAINZ_USER_AGENT: "Offbeat/Test",
		};
		const message = queueMessage(messageBody);

		await handleArtistEnrichmentQueue(queueBatch(message), env);

		expect(main.markArtistEnrichmentFailure).toHaveBeenCalledWith(
			messageBody.jobId,
			"temporary storage failure",
		);
		expect(message.retry).toHaveBeenCalledWith({ delaySeconds: 30 });
		expect(message.ack).not.toHaveBeenCalled();
	});
});
