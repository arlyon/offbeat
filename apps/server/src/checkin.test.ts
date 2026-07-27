import { ed25519 } from "@noble/curves/ed25519.js";
import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

let worker: Unstable_DevWorker;

/** Generate an Ed25519 keypair and return hex-encoded keys. */
function generateKeypair() {
	const { secretKey, publicKey } = ed25519.keygen();
	return {
		secretKey: bytesToHex(secretKey),
		publicKey: bytesToHex(publicKey),
		secretKeyBytes: secretKey,
		publicKeyBytes: publicKey,
	};
}

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

/** Register a user via WebAuthn dev bypass and return the attestation. */
async function registerUser(pubKeyHex: string): Promise<{
	message: string;
	signature: string;
	issuer: string;
}> {
	// Begin registration to get a challenge
	const beginResp = await worker.fetch("/auth/register/begin", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ userId: `checkin-test-${pubKeyHex.slice(0, 8)}` }),
	});
	expect(beginResp.status).toBe(200);
	const { challenge } = (await beginResp.json()) as { challenge: string };

	// Complete registration with dev bypass
	const completeResp = await worker.fetch("/auth/register/complete", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			webauthnResponse: {},
			challenge,
			ed25519PublicKey: pubKeyHex,
		}),
	});
	expect(completeResp.status).toBe(200);
	const data = (await completeResp.json()) as {
		attestation: { message: string; signature: string; issuer: string };
	};
	return data.attestation;
}

/** Build auth headers for a checkin request. */
function buildAuthHeaders(
	attestation: { message: string; signature: string; issuer: string },
	secretKeyBytes: Uint8Array,
	publicKeyHex: string,
): Record<string, string> {
	const timestamp = Math.floor(Date.now() / 1000).toString();
	const sessionMsg = new TextEncoder().encode(`session:${timestamp}`);
	const sessionSig = ed25519.sign(sessionMsg, secretKeyBytes);
	return {
		"X-Attestation-Message": attestation.message,
		"X-Attestation-Signature": attestation.signature,
		"X-Attestation-Issuer": attestation.issuer,
		"X-Session-PublicKey": publicKeyHex,
		"X-Session-Signature": bytesToHex(sessionSig),
		"X-Session-Timestamp": timestamp,
	};
}

const FESTIVAL_ID = "checkin-test-fest";

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

	// Configure the Festival DO directly so checkin works
	// (bypasses MainDO festival lookup by setting config + festivalId on the DO)
	const configResp = await worker.fetch(`/festivals/${FESTIVAL_ID}/config`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			opensAt: "2020-01-01T00:00:00.000Z",
			closesAt: "2030-12-31T23:59:59.999Z",
			festivalId: FESTIVAL_ID,
			lat: 51.5,
			lon: -0.1,
		}),
	});
	expect(configResp.status).toBe(200);
});

afterAll(async () => {
	await worker.stop();
});

describe("POST /festivals/:id/checkin", () => {
	it("returns 401 without auth headers", async () => {
		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json" },
			body: JSON.stringify({
				endpoint_id: "a".repeat(64),
				relay_url: null,
			}),
		});
		expect(resp.status).toBe(401);
		const text = await resp.text();
		expect(text).toContain("Auth headers required");
	});

	it("returns 400 for invalid endpoint_id (too short)", async () => {
		const kp = generateKeypair();
		const attestation = await registerUser(kp.publicKey);
		const headers = buildAuthHeaders(attestation, kp.secretKeyBytes, kp.publicKey);

		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify({
				endpoint_id: "abc123",
				relay_url: null,
			}),
		});
		expect(resp.status).toBe(400);
		const text = await resp.text();
		expect(text).toContain("endpoint_id must be exactly 64 hex characters");
	});

	it("returns 400 for invalid endpoint_id (non-hex)", async () => {
		const kp = generateKeypair();
		const attestation = await registerUser(kp.publicKey);
		const headers = buildAuthHeaders(attestation, kp.secretKeyBytes, kp.publicKey);

		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify({
				endpoint_id: "g".repeat(64),
				relay_url: null,
			}),
		});
		expect(resp.status).toBe(400);
	});

	it("returns 400 for missing endpoint_id", async () => {
		const kp = generateKeypair();
		const attestation = await registerUser(kp.publicKey);
		const headers = buildAuthHeaders(attestation, kp.secretKeyBytes, kp.publicKey);

		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify({
				relay_url: null,
			}),
		});
		expect(resp.status).toBe(400);
	});

	it("succeeds with valid auth and endpoint_id", async () => {
		const kp = generateKeypair();
		const attestation = await registerUser(kp.publicKey);
		const headers = buildAuthHeaders(attestation, kp.secretKeyBytes, kp.publicKey);

		const endpointId = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify({
				endpoint_id: endpointId,
				relay_url: "https://relay.example.com",
			}),
		});
		expect(resp.status).toBe(200);
		const data = (await resp.json()) as { ttl: number; peer_count: number };
		expect(data.ttl).toBe(7200);
		expect(data.peer_count).toBeGreaterThanOrEqual(1);
	});

	it("succeeds with null relay_url", async () => {
		const kp = generateKeypair();
		const attestation = await registerUser(kp.publicKey);
		const headers = buildAuthHeaders(attestation, kp.secretKeyBytes, kp.publicKey);

		const endpointId = "b1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify({
				endpoint_id: endpointId,
				relay_url: null,
			}),
		});
		expect(resp.status).toBe(200);
		const data = (await resp.json()) as { ttl: number; peer_count: number };
		expect(data.ttl).toBe(7200);
		expect(data.peer_count).toBeGreaterThanOrEqual(1);
	});

	it("updates peer_count when multiple peers check in", async () => {
		// First peer
		const kp1 = generateKeypair();
		const att1 = await registerUser(kp1.publicKey);
		const headers1 = buildAuthHeaders(att1, kp1.secretKeyBytes, kp1.publicKey);
		const eid1 = "c1c2c3c4c5c6c7c8c9cacbcccdcecfc1c2c3c4c5c6c7c8c9cacbcccdcecfc1c2";

		const resp1 = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers1 },
			body: JSON.stringify({ endpoint_id: eid1, relay_url: null }),
		});
		expect(resp1.status).toBe(200);
		const data1 = (await resp1.json()) as { peer_count: number };

		// Second peer
		const kp2 = generateKeypair();
		const att2 = await registerUser(kp2.publicKey);
		const headers2 = buildAuthHeaders(att2, kp2.secretKeyBytes, kp2.publicKey);
		const eid2 = "d1d2d3d4d5d6d7d8d9dadbdcdddedfd1d2d3d4d5d6d7d8d9dadbdcdddedfd1d2";

		const resp2 = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers2 },
			body: JSON.stringify({ endpoint_id: eid2, relay_url: null }),
		});
		expect(resp2.status).toBe(200);
		const data2 = (await resp2.json()) as { peer_count: number };
		expect(data2.peer_count).toBeGreaterThan(data1.peer_count);
	});

	it("returns 401 with invalid session signature", async () => {
		const kp = generateKeypair();
		const attestation = await registerUser(kp.publicKey);

		// Use a different key to sign the session (wrong key)
		const wrongKp = generateKeypair();
		const headers = buildAuthHeaders(attestation, wrongKp.secretKeyBytes, kp.publicKey);

		const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/checkin`, {
			method: "POST",
			headers: { "Content-Type": "application/json", ...headers },
			body: JSON.stringify({
				endpoint_id: "e".repeat(64),
				relay_url: null,
			}),
		});
		expect(resp.status).toBe(401);
		const text = await resp.text();
		expect(text).toContain("Invalid session signature");
	});
});
