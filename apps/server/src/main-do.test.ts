import { ed25519 } from "@noble/curves/ed25519.js";
import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const ADMIN_SECRET = new Uint8Array(32).fill(7);
const ADMIN_PUBLIC_HEX = bytesToHex(ed25519.getPublicKey(ADMIN_SECRET));

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function artistAdminHeaders(
	method: string,
	path: string,
	body = "",
	nonce = crypto.randomUUID(),
): Promise<Record<string, string>> {
	const timestamp = String(Math.floor(Date.now() / 1000));
	const digest = new Uint8Array(
		await crypto.subtle.digest("SHA-256", new TextEncoder().encode(body)),
	);
	const message = `${method}\n${path}\n${timestamp}\n${nonce}\n${bytesToHex(digest)}`;
	return {
		"Content-Type": "application/json",
		"X-Admin-Key": ADMIN_PUBLIC_HEX,
		"X-Admin-Sig": bytesToHex(ed25519.sign(new TextEncoder().encode(message), ADMIN_SECRET)),
		"X-Admin-Timestamp": timestamp,
		"X-Admin-Nonce": nonce,
	};
}

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

		it("protects artist resolution review and backfill with admin authentication", async () => {
			const review = await worker.fetch("/festivals/nonexistent/artist-resolutions");
			expect(review.status).toBe(401);
			const backfill = await worker.fetch("/artist-resolutions/backfill", { method: "POST" });
			expect(backfill.status).toBe(401);
		});

		it("rejects oversized artist override bodies at the public worker boundary", async () => {
			const response = await worker.fetch("/festivals/nonexistent/artist-resolutions", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: "x".repeat(70 * 1024),
			});
			expect(response.status).toBe(413);
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

			const resp = await worker.fetch("/admins", {
				method: "PUT",
				headers: { "Content-Type": "application/json" },
				body: JSON.stringify({ publicKey: ADMIN_PUBLIC_HEX }),
			});
			expect(resp.status).toBe(200);
			const data = (await resp.json()) as { ok: boolean };
			expect(data.ok).toBe(true);
		});

		it("binds artist admin requests to method, path, body, timestamp, and nonce", async () => {
			const path = "/festivals/nonexistent/artist-resolutions";
			const nonce = crypto.randomUUID();
			const headers = await artistAdminHeaders("GET", path, "", nonce);
			const first = await worker.fetch(path, { headers });
			expect(first.status).toBe(404);
			const replay = await worker.fetch(path, { headers });
			expect(replay.status).toBe(409);

			const crossMethodHeaders = await artistAdminHeaders("GET", path);
			const crossMethod = await worker.fetch(path, {
				method: "PUT",
				headers: crossMethodHeaders,
				body: JSON.stringify({ billingKey: "name:test", credits: [] }),
			});
			expect(crossMethod.status).toBe(401);

			const signedBody = JSON.stringify({ billingKey: "name:test", credits: [] });
			const substitutedBody = JSON.stringify({ billingKey: "name:other", credits: [] });
			const bodyHeaders = await artistAdminHeaders("PUT", path, signedBody);
			const substituted = await worker.fetch(path, {
				method: "PUT",
				headers: bodyHeaders,
				body: substitutedBody,
			});
			expect(substituted.status).toBe(401);
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
