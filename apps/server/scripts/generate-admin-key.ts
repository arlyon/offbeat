#!/usr/bin/env npx tsx
/**
 * Generate an Ed25519 keypair for admin authentication.
 *
 * Usage:
 *   npx tsx scripts/generate-admin-key.ts
 *
 * Output:
 *   Prints the secret key (for ADMIN_SECRET_KEY env var)
 *   and public key (to register as admin in the database).
 */

import { ed25519 } from "@noble/curves/ed25519.js";

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

const secretKey = ed25519.utils.randomSecretKey();
const publicKey = ed25519.getPublicKey(secretKey);

console.log("Generated Ed25519 keypair for admin authentication:\n");
console.log("Secret Key (keep private, use as ADMIN_SECRET_KEY):");
console.log(`  ${bytesToHex(secretKey)}\n`);
console.log("Public Key (register this in the admins table):");
console.log(`  ${bytesToHex(publicKey)}\n`);
console.log("To use:");
console.log("  1. Add public key to admins table in the database");
console.log("  2. Set ADMIN_SECRET_KEY env var when running scripts");
console.log("\nExample:");
console.log(`  ADMIN_SECRET_KEY=${bytesToHex(secretKey)} npx tsx scripts/register-festival.ts myfest.json`);
