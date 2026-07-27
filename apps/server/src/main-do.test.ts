import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

let worker: Unstable_DevWorker;

beforeAll(async () => {
	worker = await unstable_dev("src/index.ts", {
		persist: false,
		experimental: { disableExperimentalWarning: true },
	});

	// Warmup
	let ready = false;
	for (let i = 0; i < 10 && !ready; i++) {
		try {
			const resp = await worker.fetch("/festivals");
			if (resp.ok) ready = true;
		} catch {
			await new Promise((r) => setTimeout(r, 500));
		}
	}
});

afterAll(async () => {
	await worker.stop();
});

describe("MainDO API", () => {
	describe("festivals", () => {
		it("GET /festivals returns the server-authoritative registry", async () => {
			const resp = await worker.fetch("/festivals");
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as unknown;
			expect(Array.isArray(data)).toBe(true);
		});

		it("GET /festivals/:id returns 404 for an unknown festival", async () => {
			const resp = await worker.fetch("/festivals/nonexistent");
			expect(resp.status).toBe(404);
		});

		it("GET /festivals/:id/lineup returns 404 for an unknown festival", async () => {
			const resp = await worker.fetch("/festivals/nonexistent/lineup");
			expect(resp.status).toBe(404);
		});
	});

	describe("auth", () => {
		it("GET /auth/public-key returns a hex string", async () => {
			const resp = await worker.fetch("/auth/public-key");
			expect(resp.status).toBe(200);
			const key = await resp.text();
			expect(key).toMatch(/^[0-9a-f]{64}$/);
		});

		it("POST /auth/register/begin returns challenge + options", async () => {
			const resp = await worker.fetch("/auth/register/begin", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ userId: "test-user" }),
			});
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as { challenge: string; rp: { id: string } };
			expect(data.challenge).toBeDefined();
			expect(typeof data.challenge).toBe("string");
			expect(data.challenge.length).toBeGreaterThan(0);
			expect(data.rp.id).toBeDefined();
		});

		it("POST /auth/register/complete rejects missing challenge", async () => {
			const resp = await worker.fetch("/auth/register/complete", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					webauthnResponse: {},
					ed25519PublicKey: "a".repeat(64),
				}),
			});
			expect(resp.status).toBe(400);
			const text = await resp.text();
			expect(text).toContain("challenge");
		});

		it("POST /auth/register/complete rejects invalid ed25519 key", async () => {
			const resp = await worker.fetch("/auth/register/complete", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					webauthnResponse: {},
					challenge: "fake-challenge",
					ed25519PublicKey: "tooshort",
				}),
			});
			expect(resp.status).toBe(400);
			const text = await resp.text();
			expect(text).toContain("64 hex chars");
		});

		it("POST /auth/register/complete rejects expired/unknown challenge", async () => {
			const resp = await worker.fetch("/auth/register/complete", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					webauthnResponse: {},
					challenge: "nonexistent-challenge",
					ed25519PublicKey: "a".repeat(64),
				}),
			});
			expect(resp.status).toBe(400);
			const text = await resp.text();
			expect(text).toContain("Invalid or expired challenge");
		});

		it("register/begin challenge can be consumed once by register/complete", async () => {
			// Get a real challenge
			const beginResp = await worker.fetch("/auth/register/begin", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ userId: "test-user-challenge" }),
			});
			expect(beginResp.status).toBe(200);
			const { challenge } = (await beginResp.json()) as { challenge: string };

			// First attempt will fail verification but consume the challenge
			const firstResp = await worker.fetch("/auth/register/complete", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					webauthnResponse: { id: "fake", rawId: "fake", type: "public-key", response: {} },
					challenge,
					ed25519PublicKey: "a".repeat(64),
				}),
			});
			// DEV_BYPASS_WEBAUTHN accepts the first registration in integration tests.
			expect(firstResp.status).toBe(200);

			// Second attempt with same challenge should fail as expired
			const secondResp = await worker.fetch("/auth/register/complete", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({
					webauthnResponse: { id: "fake", rawId: "fake", type: "public-key", response: {} },
					challenge,
					ed25519PublicKey: "a".repeat(64),
				}),
			});
			expect(secondResp.status).toBe(400);
			const text = await secondResp.text();
			expect(text).toContain("Invalid or expired challenge");
		});
	});

	describe("admins", () => {
		it("GET /admins returns an array", async () => {
			const resp = await worker.fetch("/admins");
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as string[];
			expect(Array.isArray(data)).toBe(true);
		});

		it("PUT /admins rejects invalid key length", async () => {
			const resp = await worker.fetch("/admins", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ publicKey: "tooshort" }),
			});
			expect(resp.status).toBe(400);
		});

		it("PUT /admins bootstrap works when no admins exist", async () => {
			// First check if admins already exist (from previous test runs)
			const listResp = await worker.fetch("/admins");
			const existing = (await listResp.json()) as string[];

			if (existing.length > 0) {
				// Already bootstrapped — skip
				return;
			}

			const key = "b".repeat(64);
			const resp = await worker.fetch("/admins", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ publicKey: key }),
			});
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as { ok: boolean };
			expect(data.ok).toBe(true);
		});

		it("PUT /admins rejects second admin without auth headers", async () => {
			// Ensure at least one admin exists first
			const listResp = await worker.fetch("/admins");
			const existing = (await listResp.json()) as string[];
			if (existing.length === 0) {
				// Bootstrap first
				await worker.fetch("/admins", {
					method: "PUT",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ publicKey: "b".repeat(64) }),
				});
			}

			// Now try adding another without auth
			const resp = await worker.fetch("/admins", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ publicKey: "c".repeat(64) }),
			});
			expect(resp.status).toBe(401);
		});
	});

	describe("admin requests", () => {
		it("POST /admins/request creates a pending request", async () => {
			const key = "d".repeat(64);
			const resp = await worker.fetch("/admins/request", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ publicKey: key, displayName: "Test User" }),
			});
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as { status: string };
			expect(data.status).toBe("pending");
		});

		it("GET /admins/requests lists pending requests", async () => {
			const resp = await worker.fetch("/admins/requests");
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as {
				publicKey: string;
				displayName: string;
				requestedAt: string;
			}[];
			expect(Array.isArray(data)).toBe(true);
			const found = data.find((r) => r.publicKey === "d".repeat(64));
			expect(found).toBeDefined();
			expect(found?.displayName).toBe("Test User");
		});

		it("POST /admins/request returns already_admin for existing admins", async () => {
			// Ensure admin exists
			const listResp = await worker.fetch("/admins");
			const existing = (await listResp.json()) as string[];
			if (existing.length === 0) {
				await worker.fetch("/admins", {
					method: "PUT",
					headers: { "Content-Type": "application/json" },
					body: JSON.stringify({ publicKey: "b".repeat(64) }),
				});
			}
			const admins = (await (await worker.fetch("/admins")).json()) as string[];
			const adminKey = admins[0];

			const resp = await worker.fetch("/admins/request", {
				method: "POST",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ publicKey: adminKey }),
			});
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as { status: string };
			expect(data.status).toBe("already_admin");
		});

		it("POST /admins/requests/:key/approve requires admin auth", async () => {
			const key = "d".repeat(64);
			const resp = await worker.fetch(`/admins/requests/${key}/approve`, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
			});
			expect(resp.status).toBe(401);
		});

		it("POST /admins/requests/:key/deny requires admin auth", async () => {
			const key = "d".repeat(64);
			const resp = await worker.fetch(`/admins/requests/${key}/deny`, {
				method: "POST",
				headers: { "Content-Type": "application/json" },
			});
			expect(resp.status).toBe(401);
		});
	});

	describe(".well-known", () => {
		it("GET /.well-known/assetlinks.json returns Android asset links", async () => {
			const resp = await worker.fetch("/.well-known/assetlinks.json");
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as {
				relation: string[];
				target: { namespace: string; package_name: string; sha256_cert_fingerprints: string[] };
			}[];
			expect(data.length).toBe(1);
			expect(data[0].target.namespace).toBe("android_app");
			expect(data[0].target.package_name).toBe("com.offbeat.offbeat_mobile");
			expect(data[0].target.sha256_cert_fingerprints.length).toBe(1);
		});
	});
});
