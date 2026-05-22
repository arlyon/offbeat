#!/usr/bin/env npx tsx
/**
 * Seed festivals from a folder of JSON files via Clashfinder.
 *
 * Usage:
 *   npx tsx scripts/seed-festivals.ts [folder] [options]
 *
 * Arguments:
 *   folder             Path to folder containing festival JSON files (default: fixtures/)
 *
 * Options:
 *   --api-url <url>    API base URL (default: http://localhost:8787)
 *   --dry-run          Parse and validate without making requests
 *
 * Environment:
 *   ADMIN_SECRET_KEY   64-char hex secret key for admin auth
 */

import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { ed25519 } from "@noble/curves/ed25519.js";

interface FestivalFixture {
	id: string;
	name: string;
	location: string;
	city: string;
	country: string;
	genres: string[];
	clashfinderId: string;
	lat?: number;
	lon?: number;
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

async function registerFestival(
	festival: FestivalFixture,
	apiUrl: string,
	secretKey: Uint8Array,
): Promise<{ success: boolean; error?: string }> {
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
			lat: festival.lat,
			lon: festival.lon,
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
		return { success: false, error: `${res.status} ${text}` };
	}

	return { success: true };
}

async function main() {
	const args = process.argv.slice(2);

	if (args.includes("--help") || args.includes("-h")) {
		console.log(`
Usage: npx tsx scripts/seed-festivals.ts [folder] [options]

Arguments:
  folder               Path to folder with festival JSON files (default: fixtures/)

Options:
  --api-url <url>      API base URL (default: http://localhost:8787)
  --dry-run            Parse and validate without making requests

Environment:
  ADMIN_SECRET_KEY     64-char hex secret key for admin auth

Example:
  ADMIN_SECRET_KEY=abc... npx tsx scripts/seed-festivals.ts
  ADMIN_SECRET_KEY=abc... npx tsx scripts/seed-festivals.ts ./my-festivals/
`);
		process.exit(0);
	}

	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";
	const dryRun = args.includes("--dry-run");

	// Find folder argument (first arg that doesn't start with --)
	const folderArg = args.find(
		(a, i) => !a.startsWith("--") && (i === 0 || !args[i - 1]?.startsWith("--api")),
	);
	const fixturesDir = resolve(folderArg || "fixtures");

	const secretKeyHex = process.env.ADMIN_SECRET_KEY;
	if (!secretKeyHex && !dryRun) {
		console.error("Error: ADMIN_SECRET_KEY environment variable required");
		console.error("Run: pnpm -F @offbeat/server admin:bootstrap");
		process.exit(1);
	}

	// Read all JSON files from the folder
	let files: string[];
	try {
		files = readdirSync(fixturesDir).filter((f) => f.endsWith(".json"));
	} catch (err) {
		console.error(`Error reading directory ${fixturesDir}:`, err instanceof Error ? err.message : err);
		process.exit(1);
	}

	if (files.length === 0) {
		console.log(`No JSON files found in ${fixturesDir}`);
		process.exit(0);
	}

	console.log(`Found ${files.length} festival file(s) in ${fixturesDir}\n`);

	const festivals: FestivalFixture[] = [];
	for (const file of files) {
		const filePath = join(fixturesDir, file);
		try {
			const content = readFileSync(filePath, "utf-8");
			const festival = JSON.parse(content) as FestivalFixture;

			// Validate required fields
			const required = ["id", "name", "location", "city", "country", "genres", "clashfinderId"];
			const missing = required.filter((f) => !(f in festival));
			if (missing.length > 0) {
				console.error(`  [${file}] Missing required fields: ${missing.join(", ")}`);
				continue;
			}

			festivals.push(festival);
			console.log(`  [${file}] ${festival.name}`);
			console.log(`           ${festival.location}`);
			console.log(`           Clashfinder: ${festival.clashfinderId}`);
		} catch (err) {
			console.error(`  [${file}] Parse error:`, err instanceof Error ? err.message : err);
		}
	}

	if (dryRun) {
		console.log(`\n[Dry run] Would register ${festivals.length} festival(s) via Clashfinder`);
		return;
	}

	const secretKey = hexToBytes(secretKeyHex!);

	console.log(`\nRegistering ${festivals.length} festival(s) via Clashfinder...\n`);

	let success = 0;
	let failed = 0;

	for (const festival of festivals) {
		process.stdout.write(`  ${festival.id}... `);
		const result = await registerFestival(festival, apiUrl, secretKey);
		if (result.success) {
			console.log("OK");
			success++;
		} else {
			console.log(`FAILED: ${result.error}`);
			failed++;
		}
	}

	console.log(`\nDone: ${success} succeeded, ${failed} failed`);
	if (failed > 0) process.exit(1);
}

main().catch((err) => {
	console.error("Unexpected error:", err);
	process.exit(1);
});
