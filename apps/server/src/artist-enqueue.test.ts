import { describe, expect, it, vi } from "vitest";
import { enqueueArtistEnrichment } from "./api";
import type { ArtistEnrichmentMessage } from "./artist-enrichment";

function candidate(index: number, contextBillings: string[] = []): ArtistEnrichmentMessage {
	return {
		jobId: `job-${index}`,
		sourceKey: `name:artist-${index}`,
		festivalId: "festival-1",
		setIds: [`set-${index}`],
		billing: `Artist ${index}`,
		billingKey: `name:artist-${index}`,
		contextBillings,
	};
}

describe("artist enrichment enqueue accounting", () => {
	it("keeps lineup-aware batches below the Cloudflare total-size limit", async () => {
		const contextBillings = Array.from({ length: 80 }, (_, index) =>
			`Context ${index}`.padEnd(400, "x"),
		);
		const candidates = Array.from({ length: 12 }, (_, index) =>
			candidate(index, contextBillings),
		);
		const main = {
			getArtistEnrichmentCandidates: vi.fn(async () => candidates),
			markArtistEnrichmentQueued: vi.fn(async () => undefined),
		};
		const sendBatch = vi.fn(async (_messages: Array<{ body: ArtistEnrichmentMessage }>) => undefined);
		const env = {
			MAIN_DO: {
				idFromName: vi.fn(() => "main-id"),
				get: vi.fn(() => main),
			},
			ARTIST_ENRICHMENT_QUEUE: { sendBatch },
		};

		const result = await enqueueArtistEnrichment(env as never, "festival-1");

		expect(result).toEqual({ queuedJobs: candidates.length, complete: true });
		expect(sendBatch.mock.calls.length).toBeGreaterThan(1);
		const encoder = new TextEncoder();
		for (const [messages] of sendBatch.mock.calls) {
			expect(messages.length).toBeLessThanOrEqual(100);
			expect(encoder.encode(JSON.stringify(messages)).byteLength).toBeLessThan(256 * 1024);
		}
		expect(sendBatch.mock.calls.flatMap(([messages]) => messages)).toHaveLength(candidates.length);
	});

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
		const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);

		const result = await enqueueArtistEnrichment(env as never, "festival-1");

		expect(result).toEqual({ queuedJobs: 100, complete: false });
		expect(consoleError).toHaveBeenCalledWith(
			"[artist-enrichment] failed to send queue batch",
			expect.any(Error),
		);
		consoleError.mockRestore();
		expect(sendBatch).toHaveBeenCalledTimes(2);
		expect(main.markArtistEnrichmentQueued).toHaveBeenCalledTimes(1);
		expect(main.markArtistEnrichmentQueued).toHaveBeenCalledWith(
			candidates.slice(0, 100).map((item) => item.jobId),
		);
	});
});
