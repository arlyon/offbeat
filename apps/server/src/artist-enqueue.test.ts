import { describe, expect, it, vi } from "vitest";
import { enqueueArtistEnrichment } from "./api";
import type { ArtistEnrichmentMessage } from "./artist-enrichment";

function candidate(index: number): ArtistEnrichmentMessage {
	return {
		jobId: `job-${index}`,
		sourceKey: `name:artist-${index}`,
		festivalId: "festival-1",
		setIds: [`set-${index}`],
		billing: `Artist ${index}`,
		billingKey: `name:artist-${index}`,
		contextBillings: [],
	};
}

describe("artist enrichment enqueue accounting", () => {
	it("reports jobs already sent when a later batch fails", async () => {
		const candidates = Array.from({ length: 150 }, (_, index) => candidate(index));
		const main = {
			getArtistEnrichmentCandidates: vi.fn(async () => candidates),
			markArtistEnrichmentQueued: vi.fn(async () => undefined),
		};
		const sendBatch = vi
			.fn()
			.mockResolvedValueOnce(undefined)
			.mockRejectedValueOnce(new Error("queue unavailable"));
		const env = {
			MAIN_DO: {
				idFromName: vi.fn(() => "main-id"),
				get: vi.fn(() => main),
			},
			ARTIST_ENRICHMENT_QUEUE: { sendBatch },
		};

		const result = await enqueueArtistEnrichment(env as never, "festival-1");

		expect(result).toEqual({ queuedJobs: 100, complete: false });
		expect(sendBatch).toHaveBeenCalledTimes(2);
		expect(main.markArtistEnrichmentQueued).toHaveBeenCalledTimes(1);
		expect(main.markArtistEnrichmentQueued).toHaveBeenCalledWith(
			candidates.slice(0, 100).map((item) => item.jobId),
		);
	});
});
