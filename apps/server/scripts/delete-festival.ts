#!/usr/bin/env npx tsx
/**
 * Delete a festival by ID.
 *
 * Usage:
 *   npx tsx scripts/delete-festival.ts <festival-id> [options]
 *
 * Options:
 *   --api-url <url>    API base URL (default: http://localhost:8787)
 *
 * Environment:
 *   ADMIN_SECRET_KEY   64-char hex secret key for admin auth
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

async function main() {
	const args = process.argv.slice(2);

	if (args.length === 0 || args.includes("--help") || args.includes("-h")) {
		console.log(`
Usage: npx tsx scripts/delete-festival.ts <festival-id> [options]

Options:
  --api-url <url>    API base URL (default: http://localhost:8787)

Environment:
  ADMIN_SECRET_KEY   64-char hex secret key for admin auth

Example:
  ADMIN_SECRET_KEY=abc... npx tsx scripts/delete-festival.ts fieldday2026
`);
		process.exit(0);
	}

	const festivalId = args.find(
		(a, i) => !a.startsWith("--") && (i === 0 || !args[i - 1]?.startsWith("--api")),
	);
	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";

	if (!festivalId) {
		console.error("Error: No festival ID specified");
		process.exit(1);
	}

	const secretKeyHex = process.env.ADMIN_SECRET_KEY;
	if (!secretKeyHex) {
		console.error("Error: ADMIN_SECRET_KEY environment variable required");
		process.exit(1);
	}

	const secretKey = hexToBytes(secretKeyHex);
	const path = `/festivals/${festivalId}`;
	const { pubKey, signature } = signRequest(secretKey, path);

	console.log(`Deleting festival: ${festivalId}...`);

	const res = await fetch(`${apiUrl}${path}`, {
		method: "DELETE",
		headers: {
			"X-Admin-Key": pubKey,
			"X-Admin-Sig": signature,
		},
	});

	if (res.status === 204) {
		console.log("Festival deleted successfully");
	} else if (res.status === 404) {
		console.error("Festival not found");
		process.exit(1);
	} else {
		const text = await res.text();
		console.error(`Failed to delete festival: ${res.status} ${text}`);
		process.exit(1);
	}
}

main().catch((err) => {
	console.error("Unexpected error:", err);
	process.exit(1);
});
