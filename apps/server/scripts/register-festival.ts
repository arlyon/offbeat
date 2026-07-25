#!/usr/bin/env npx tsx
/**
 * Register a single festival from a JSON file via Clashfinder.
 *
 * Usage:
 *   npx tsx scripts/register-festival.ts <festival.json> [options]
 *
 * Options:
 *   --api-url <url>    API base URL (default: http://localhost:8787)
 *   --dry-run          Parse and validate without making requests
 *
 * Environment:
 *   ADMIN_SECRET_KEY   64-char hex secret key for admin auth
 *
 * JSON file format:
 * {
 *   "id": "myfestival2026",
 *   "name": "My Festival 2026",
 *   "location": "Victoria Park, London",
 *   "city": "London",
 *   "country": "GB",
 *   "genres": ["Electronic", "Indie"],
 *   "clashfinderId": "myfestival2026"
 * }
 */

import { readFileSync } from "node:fs";
import { ed25519 } from "@noble/curves/ed25519.js";

interface FestivalInput {
	id: string;
	name: string;
	location: string;
	city: string;
	country: string;
	genres: string[];
	clashfinderId: string;
}

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

async function main() {
	const args = process.argv.slice(2);

	if (args.length === 0 || args.includes("--help") || args.includes("-h")) {
		console.log(`
Usage: npx tsx scripts/register-festival.ts <festival.json> [options]

Options:
  --api-url <url>    API base URL (default: http://localhost:8787)
  --dry-run          Parse and validate without making requests

Environment:
  ADMIN_SECRET_KEY   64-char hex secret key for admin auth

Example:
  ADMIN_SECRET_KEY=abc123... npx tsx scripts/register-festival.ts fixtures/fieldday2026.json
`);
		process.exit(0);
	}

	const jsonFile = args.find(
		(a) => !a.startsWith("--") && !args[args.indexOf(a) - 1]?.startsWith("--api"),
	);
	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";
	const dryRun = args.includes("--dry-run");

	if (!jsonFile) {
		console.error("Error: No JSON file specified");
		process.exit(1);
	}

	const secretKeyHex = process.env.ADMIN_SECRET_KEY;
	if (!secretKeyHex && !dryRun) {
		console.error("Error: ADMIN_SECRET_KEY environment variable required");
		console.error("Generate one with: pnpm -F @offbeat/server admin:bootstrap");
		process.exit(1);
	}

	// Read and parse JSON
	let festival: FestivalInput;
	try {
		const content = readFileSync(jsonFile, "utf-8");
		festival = JSON.parse(content);
	} catch (err) {
		console.error(`Error reading ${jsonFile}:`, err instanceof Error ? err.message : err);
		process.exit(1);
	}

	// Validate required fields
	const required = ["id", "name", "location", "city", "country", "genres", "clashfinderId"];
	const missing = required.filter((f) => !(f in festival));
	if (missing.length > 0) {
		console.error(`Error: Missing required fields: ${missing.join(", ")}`);
		process.exit(1);
	}

	console.log(`Festival: ${festival.name} (${festival.id})`);
	console.log(`  Location: ${festival.location}, ${festival.city}, ${festival.country}`);
	console.log(`  Clashfinder: ${festival.clashfinderId}`);
	console.log(`  Genres: ${festival.genres.join(", ")}`);

	if (dryRun) {
		console.log("\n[Dry run] Would register festival via Clashfinder");
		return;
	}

	const secretKey = hexToBytes(secretKeyHex!);

	console.log("\nRegistering festival via Clashfinder...");
	const path = "/festivals";
	const { pubKey, signature } = signRequest(secretKey, path);

	const body = {
		source: {
			festivalId: festival.id,
			clashfinderId: festival.clashfinderId,
			name: festival.name,
			location: festival.location,
			city: festival.city,
			country: festival.country,
			genres: festival.genres,
		},
	};

	const res = await fetch(`${apiUrl}${path}`, {
		method: "POST",
		headers: {
			"Content-Type": "application/json",
			"X-Admin-Key": pubKey,
			"X-Admin-Sig": signature,
		},
		body: JSON.stringify(body),
	});

	if (!res.ok) {
		const text = await res.text();
		console.error(`Failed to register festival: ${res.status} ${text}`);
		process.exit(1);
	}

	console.log(`\nDone! Festival registered: ${apiUrl}/festivals/${festival.id}`);
}

main().catch((err) => {
	console.error("Unexpected error:", err);
	process.exit(1);
});
