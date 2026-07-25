#!/usr/bin/env npx tsx
/**
 * Add a new admin after bootstrap.
 *
 * Usage:
 *   npx tsx scripts/add-admin.ts <public-key> [options]
 *   npx tsx scripts/add-admin.ts --generate [options]
 *
 * Arguments:
 *   public-key         64-char hex public key to register as admin
 *
 * Options:
 *   --generate         Generate a new keypair and register it
 *   --api-url <url>    API base URL (default: http://localhost:8787)
 *
 * Environment:
 *   ADMIN_SECRET_KEY   64-char hex secret key of existing admin (for auth)
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
Usage: npx tsx scripts/add-admin.ts <public-key> [options]
       npx tsx scripts/add-admin.ts --generate [options]

Arguments:
  public-key           64-char hex public key to register as admin

Options:
  --generate           Generate a new keypair and register it
  --api-url <url>      API base URL (default: http://localhost:8787)

Environment:
  ADMIN_SECRET_KEY     64-char hex secret key of existing admin (for auth)

Examples:
  # Add an existing public key
  ADMIN_SECRET_KEY=abc... npx tsx scripts/add-admin.ts def456...

  # Generate and add a new admin
  ADMIN_SECRET_KEY=abc... npx tsx scripts/add-admin.ts --generate
`);
		process.exit(0);
	}

	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";
	const generate = args.includes("--generate");

	const secretKeyHex = process.env.ADMIN_SECRET_KEY;
	if (!secretKeyHex) {
		console.error("Error: ADMIN_SECRET_KEY environment variable required");
		console.error("This must be the secret key of an existing admin.");
		process.exit(1);
	}

	let newPublicKey: string;
	let newSecretKey: string | undefined;

	if (generate) {
		// Generate a new keypair
		const sk = ed25519.utils.randomSecretKey();
		const pk = ed25519.getPublicKey(sk);
		newPublicKey = bytesToHex(pk);
		newSecretKey = bytesToHex(sk);
		console.log("Generated new admin keypair:\n");
	} else {
		// Use provided public key
		newPublicKey =
			args.find((a, i) => !a.startsWith("--") && (i === 0 || !args[i - 1]?.startsWith("--api"))) ||
			"";

		if (!newPublicKey || newPublicKey.length !== 64) {
			console.error("Error: Public key must be 64 hex characters");
			process.exit(1);
		}
	}

	const secretKey = hexToBytes(secretKeyHex);
	const path = "/admins";
	const { pubKey, signature } = signRequest(secretKey, path);

	console.log(`Adding admin: ${newPublicKey.slice(0, 16)}...`);

	const res = await fetch(`${apiUrl}${path}`, {
		method: "PUT",
		headers: {
			"Content-Type": "application/json",
			"X-Admin-Key": pubKey,
			"X-Admin-Sig": signature,
		},
		body: JSON.stringify({ publicKey: newPublicKey }),
	});

	if (!res.ok) {
		const text = await res.text();
		console.error(`Failed to add admin: ${res.status} ${text}`);
		process.exit(1);
	}

	console.log("Admin added successfully!\n");

	if (newSecretKey) {
		console.log("New admin credentials:\n");
		console.log("Secret Key (ADMIN_SECRET_KEY):");
		console.log(`  ${newSecretKey}\n`);
		console.log("Public Key (registered):");
		console.log(`  ${newPublicKey}\n`);
		console.log("Usage:");
		console.log(`  export ADMIN_SECRET_KEY=${newSecretKey}`);
	} else {
		console.log(`Public Key: ${newPublicKey}`);
	}
}

main().catch((err) => {
	console.error("Unexpected error:", err);
	process.exit(1);
});
