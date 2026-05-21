#!/usr/bin/env npx tsx
/**
 * Bootstrap the first admin on a fresh server.
 *
 * Usage:
 *   npx tsx scripts/bootstrap-admin.ts [--api-url <url>]
 *
 * This generates a new keypair and registers it as the first admin.
 * The first admin registration doesn't require auth (bootstrap mode).
 */

import { ed25519 } from "@noble/curves/ed25519.js";

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

async function main() {
	const args = process.argv.slice(2);
	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";

	// Check if admins already exist
	const listResp = await fetch(`${apiUrl}/admins`);
	if (!listResp.ok) {
		console.error(`Failed to check admins: ${listResp.status}`);
		process.exit(1);
	}

	const admins = (await listResp.json()) as string[];
	if (admins.length > 0) {
		console.error("Error: Admins already exist. Bootstrap is only for fresh servers.");
		console.error(`Existing admins: ${admins.length}`);
		process.exit(1);
	}

	// Generate a new keypair
	const secretKey = ed25519.utils.randomSecretKey();
	const publicKey = ed25519.getPublicKey(secretKey);
	const pubKeyHex = bytesToHex(publicKey);
	const secretKeyHex = bytesToHex(secretKey);

	console.log("Bootstrapping first admin...\n");

	// Register as first admin (no auth required)
	const resp = await fetch(`${apiUrl}/admins`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ publicKey: pubKeyHex }),
	});

	if (!resp.ok) {
		const text = await resp.text();
		console.error(`Failed to bootstrap admin: ${resp.status} ${text}`);
		process.exit(1);
	}

	console.log("Admin bootstrapped successfully!\n");
	console.log("Save these credentials securely:\n");
	console.log("Secret Key (ADMIN_SECRET_KEY):");
	console.log(`  ${secretKeyHex}\n`);
	console.log("Public Key (registered in database):");
	console.log(`  ${pubKeyHex}\n`);
	console.log("Usage:");
	console.log(`  export ADMIN_SECRET_KEY=${secretKeyHex}`);
	console.log("  npx tsx scripts/register-festival.ts myfest.json");
}

main().catch((err) => {
	console.error("Unexpected error:", err);
	process.exit(1);
});
