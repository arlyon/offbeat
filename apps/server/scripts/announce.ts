#!/usr/bin/env npx tsx

/**
 * Submit an announcement to a festival.
 *
 * Usage:
 *   npx tsx scripts/announce.ts <festival-id> <message> [options]
 *
 * Options:
 *   --api-url <url>    API base URL (default: http://localhost:8787)
 *   --priority <p>     Priority level: info | warning | urgent (default: info)
 *   --title <title>    Optional announcement title
 *
 * Environment:
 *   ADMIN_SECRET_KEY   64-char hex secret key for admin auth
 *
 * Example:
 *   ADMIN_SECRET_KEY=abc... npx tsx scripts/announce.ts glastonbury-2026 "Main stage delayed 30 min" --priority warning --title "Schedule Change"
 */

import { ed25519 } from "@noble/curves/ed25519.js";
import * as Y from "yjs";

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

function bytesToBase64(bytes: Uint8Array): string {
	let binary = "";
	for (const b of bytes) {
		binary += String.fromCharCode(b);
	}
	return btoa(binary);
}

function base64ToBytes(b64: string): Uint8Array {
	const binary = atob(b64);
	const bytes = new Uint8Array(binary.length);
	for (let i = 0; i < binary.length; i++) {
		bytes[i] = binary.charCodeAt(i);
	}
	return bytes;
}

interface Announcement {
	id: string;
	title?: string;
	message: string;
	priority: "info" | "warning" | "urgent";
	timestamp: string;
}

async function main() {
	const args = process.argv.slice(2);

	if (args.includes("--help") || args.includes("-h") || args.length < 2) {
		console.log(`
Usage: npx tsx scripts/announce.ts <festival-id> <message> [options]

Options:
  --api-url <url>    API base URL (default: http://localhost:8787)
  --priority <p>     Priority: info | warning | urgent (default: info)
  --title <title>    Optional announcement title

Environment:
  ADMIN_SECRET_KEY   64-char hex secret key for admin auth

Example:
  ADMIN_SECRET_KEY=abc... npx tsx scripts/announce.ts glastonbury-2026 "Main stage delayed 30 min" --priority warning --title "Schedule Change"
`);
		process.exit(args.length < 2 ? 1 : 0);
	}

	// Parse positional args (skip flags)
	const positional: string[] = [];
	for (let i = 0; i < args.length; i++) {
		if (args[i].startsWith("--")) {
			i++; // skip flag value
		} else {
			positional.push(args[i]);
		}
	}

	const festivalId = positional[0];
	const message = positional[1];

	if (!festivalId || !message) {
		console.error("Error: festival-id and message are required");
		process.exit(1);
	}

	const apiUrlIdx = args.indexOf("--api-url");
	const apiUrl = apiUrlIdx !== -1 ? args[apiUrlIdx + 1] : "http://localhost:8787";

	const priorityIdx = args.indexOf("--priority");
	const priority = (
		priorityIdx !== -1 ? args[priorityIdx + 1] : "info"
	) as Announcement["priority"];
	if (!["info", "warning", "urgent"].includes(priority)) {
		console.error(`Error: invalid priority "${priority}" — must be info, warning, or urgent`);
		process.exit(1);
	}

	const titleIdx = args.indexOf("--title");
	const title = titleIdx !== -1 ? args[titleIdx + 1] : undefined;

	const secretKeyHex = process.env.ADMIN_SECRET_KEY;
	if (!secretKeyHex) {
		console.error("Error: ADMIN_SECRET_KEY environment variable required");
		process.exit(1);
	}

	const secretKey = hexToBytes(secretKeyHex);
	const publicKey = ed25519.getPublicKey(secretKey);
	const pubKeyHex = bytesToHex(publicKey);

	const docId = `festival/${festivalId}/state`;
	const topic = docId;

	// Step 1: Fetch current state via sv_exchange to get existing announcements
	console.log(`Connecting to festival ${festivalId}...`);

	// We need a WebSocket connection to do sv_exchange, or we can build the update
	// from scratch. Since announcements are append-only, we'll fetch the current doc
	// state via sv_exchange over WS, read existing announcements, append ours, and
	// submit via sign-update REST endpoint.

	// Connect via WS to get current state
	const wsUrl = `${apiUrl.replace(/^http/, "ws")}/festivals/${festivalId}/ws`;
	const ws = new WebSocket(wsUrl);

	const currentState = await new Promise<Uint8Array | null>((resolve, reject) => {
		const timeout = setTimeout(() => {
			ws.close();
			reject(new Error("WebSocket timeout"));
		}, 10000);

		ws.onopen = () => {
			// Send empty state vector to get full doc
			const emptyDoc = new Y.Doc();
			const sv = Y.encodeStateVector(emptyDoc);
			ws.send(
				JSON.stringify({
					type: "sv_exchange",
					docId,
					sv: bytesToBase64(sv),
				}),
			);
		};

		ws.onmessage = (event) => {
			const data = JSON.parse(event.data as string);
			if (data.type === "sv_diff") {
				clearTimeout(timeout);
				resolve(base64ToBytes(data.diff));
			} else if (data.type === "error") {
				clearTimeout(timeout);
				reject(new Error(data.error));
			}
		};

		ws.onerror = (err) => {
			clearTimeout(timeout);
			reject(err);
		};
	});

	ws.close();

	// Build a doc with existing state
	const doc = new Y.Doc();
	if (currentState && currentState.length > 0) {
		Y.applyUpdate(doc, currentState);
	}

	// Read existing announcements
	const root = doc.getMap("root");
	const existingRaw = root.get("announcements") as string | undefined;
	const existing: Announcement[] = existingRaw ? JSON.parse(existingRaw) : [];

	// Create the new announcement
	const announcement: Announcement = {
		id: crypto.randomUUID(),
		message,
		priority,
		timestamp: new Date().toISOString(),
	};
	if (title) {
		announcement.title = title;
	}

	const updated = [...existing, announcement];

	// Capture state vector before mutation
	const svBefore = Y.encodeStateVector(doc);

	// Apply the mutation
	const txn = doc.transact(() => {
		root.set("announcements", JSON.stringify(updated));
	});

	// Encode only the diff (the new update)
	const updateBytes = Y.encodeStateAsUpdate(doc, svBefore);
	const updateBase64 = bytesToBase64(updateBytes);

	// Step 2: Sign the auth challenge and submit via sign-update
	const authMessage = new TextEncoder().encode(`sign-update:${docId}`);
	const authSig = ed25519.sign(authMessage, secretKey);

	console.log(`Submitting announcement to ${festivalId}...`);
	console.log(`  Priority: ${priority}`);
	if (title) console.log(`  Title:    ${title}`);
	console.log(`  Message:  ${message}`);

	const resp = await fetch(`${apiUrl}/festivals/${festivalId}/sign-update`, {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			publicKey: pubKeyHex,
			signature: bytesToHex(authSig),
			docId,
			topic,
			update: updateBase64,
		}),
	});

	if (!resp.ok) {
		const text = await resp.text();
		console.error(`\nFailed: ${resp.status} ${text}`);
		process.exit(1);
	}

	const result = await resp.json();
	console.log(`\nAnnouncement submitted (seq: ${(result as { seq: number }).seq})`);
	console.log(`  ID: ${announcement.id}`);
	console.log(`  Total announcements: ${updated.length}`);
}

main().catch((err) => {
	console.error("Unexpected error:", err);
	process.exit(1);
});
