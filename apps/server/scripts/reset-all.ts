#!/usr/bin/env npx tsx
/**
 * Reset all Festival DOs and re-seed festivals from fixtures.
 *
 * 1. Lists all festivals from MainDO
 * 2. Resets each Festival DO (DELETE /reset)
 * 3. Deletes each festival from MainDO (DELETE /festivals/:id)
 * 4. Re-seeds all festivals from fixture files
 *
 * Usage:
 *   ADMIN_SECRET_KEY=... npx tsx scripts/reset-all.ts [options]
 *
 * Options:
 *   --api-url <url>    API base URL (default: http://localhost:8787)
 */

import { ed25519 } from "@noble/curves/ed25519.js";

function hexToBytes(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let i = 0; i < bytes.length; i++) {
		bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
	}
	return bytes;
}

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

function signRequest(secretKey: Uint8Array, path: string): { pubKey: string; signature: string } {
	const publicKey = ed25519.getPublicKey(secretKey);
	const message = new TextEncoder().encode(path);
	const sig = ed25519.sign(message, secretKey);
	return {
		pubKey: bytesToHex(publicKey),
		signature: bytesToHex(sig),
	};
}

async function adminFetch(
	apiUrl: string,
	secretKey: Uint8Array,
	method: string,
	path: string,
	body?: unknown,
): Promise<Response> {
	const { pubKey, signature } = signRequest(secretKey, path);
	const headers: Record<string, string> = {
		"X-Admin-Key": pubKey,
		"X-Admin-Sig": signature,
	};
	if (body) headers["Content-Type"] = "application/json";
	return fetch(`${apiUrl}${path}`, {
		method,
		headers,
		body: body ? JSON.stringify(body) : undefined,
	});
}

async function main() {
	const args = process.argv.slice(2);
	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";

	const secretKeyHex = process.env.ADMIN_SECRET_KEY;
	if (!secretKeyHex) {
		console.error("Error: ADMIN_SECRET_KEY environment variable required");
		process.exit(1);
	}
	const secretKey = hexToBytes(secretKeyHex);

	// 1. List existing festivals
	console.log("Fetching existing festivals...");
	const listRes = await fetch(`${apiUrl}/festivals`);
	const festivals = (await listRes.json()) as { id: string; name: string }[];
	console.log(`  Found ${festivals.length} festival(s)`);

	// 2. Reset each Festival DO
	for (const fest of festivals) {
		process.stdout.write(`  Resetting Festival DO for ${fest.id}... `);
		const res = await adminFetch(apiUrl, secretKey, "DELETE", `/festivals/${fest.id}/reset`);
		if (res.ok) {
			console.log("OK");
		} else {
			console.log(`FAILED (${res.status})`);
		}
	}

	// 3. Delete all festivals from MainDO
	for (const fest of festivals) {
		process.stdout.write(`  Deleting ${fest.id} from MainDO... `);
		const res = await adminFetch(apiUrl, secretKey, "DELETE", `/festivals/${fest.id}`);
		if (res.ok || res.status === 204) {
			console.log("OK");
		} else {
			console.log(`FAILED (${res.status}: ${await res.text()})`);
		}
	}

	// 4. Re-seed from fixtures
	console.log("\nRe-seeding festivals...");

	// Dynamically import and run the seed script logic
	const { readdirSync, readFileSync } = await import("node:fs");
	const { join, resolve } = await import("node:path");

	const fixturesDir = resolve("fixtures");
	const files = readdirSync(fixturesDir).filter((f) => f.endsWith(".json"));

	for (const file of files) {
		const content = readFileSync(join(fixturesDir, file), "utf-8");
		const fixture = JSON.parse(content) as {
			id: string;
			name: string;
			location: string;
			city: string;
			country: string;
			genres: string[];
			clashfinderId: string;
			lat?: number;
			lon?: number;
		};

		process.stdout.write(`  Creating ${fixture.id}... `);
		const res = await adminFetch(apiUrl, secretKey, "POST", "/festivals", {
			source: {
				festivalId: fixture.id,
				clashfinderId: fixture.clashfinderId,
				name: fixture.name,
				location: fixture.location,
				city: fixture.city,
				country: fixture.country,
				genres: fixture.genres,
				lat: fixture.lat,
				lon: fixture.lon,
			},
		});

		if (res.ok) {
			console.log("OK");
		} else {
			console.log(`FAILED (${res.status}: ${await res.text()})`);
		}
	}

	console.log("\nDone.");
}

main().catch((err) => {
	console.error("Fatal:", err);
	process.exit(1);
});
