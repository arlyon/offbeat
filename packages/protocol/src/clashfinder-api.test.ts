import { afterEach, describe, expect, it, vi } from "vitest";
import { buildApiUrl, fetchClashfinder } from "./clashfinder-api";

afterEach(() => {
	vi.unstubAllGlobals();
});

describe("Clashfinder API", () => {
	it("encodes event IDs as one fixed-host path segment", () => {
		const url = buildApiUrl("../unsafe id", { username: "user", publicKey: "public" });
		expect(url).toMatch(/^https:\/\/clashfinder\.com\/data\/event\//);
		expect(url).toContain("/data/event/..%2Funsafe%20id.json?");
	});

	it("rejects redirects and cancels chunked responses at the byte limit", async () => {
		let cancelled = false;
		let chunk = 0;
		const fetchMock = vi.fn(
			async () =>
				new Response(
					new ReadableStream<Uint8Array>({
						pull(controller) {
							chunk += 1;
							controller.enqueue(new Uint8Array(20));
							if (chunk === 3) controller.close();
						},
						cancel() {
							cancelled = true;
						},
					}),
				),
		);
		vi.stubGlobal("fetch", fetchMock);
		await expect(
			fetchClashfinder(
				"event",
				{ username: "user", privateKey: "private" },
				{ maxResponseBytes: 32 },
			),
		).rejects.toThrow("response is too large");
		expect(fetchMock).toHaveBeenCalledWith(
			expect.stringMatching(/^https:\/\/clashfinder\.com\/data\/event\//),
			expect.objectContaining({ redirect: "manual" }),
		);
		expect(cancelled).toBe(true);
	});
});
