import { describe, expect, it } from "vitest";
import app from "./api";

describe("public website", () => {
	for (const [path, heading] of [
		["/", "Keep your festival on track."],
		["/support", "How can we help?"],
		["/privacy", "Privacy, without the fog."],
	] as const) {
		it(`renders ${path}`, async () => {
			const response = await app.request(`https://offbeat.arlyon.dev${path}`);
			const html = await response.text();

			expect(response.status).toBe(200);
			expect(response.headers.get("content-type")).toContain("text/html");
			expect(response.headers.get("content-security-policy")).toContain(
				"frame-ancestors 'none'",
			);
			expect(html).toContain(heading);
			expect(html).toContain(`https://offbeat.arlyon.dev${path === "/" ? "" : path}`);
		});
	}

	it("links support to the public issue tracker", async () => {
		const response = await app.request("https://offbeat.arlyon.dev/support");
		expect(await response.text()).toContain("https://github.com/arlyon/offbeat/issues");
	});
});
