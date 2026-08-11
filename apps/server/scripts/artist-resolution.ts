#!/usr/bin/env npx tsx

import { ed25519 } from "@noble/curves/ed25519.js";

declare const process: {
	argv: string[];
	env: Record<string, string | undefined>;
	stdout: { write(value: string): void };
	stderr: { write(value: string): void };
	exitCode?: number;
};

type Command = "backfill" | "list" | "override" | "retry";

interface RequestSpec {
	path: string;
	method: "GET" | "POST" | "PUT";
	body?: string;
}

const USAGE = `Usage:
  pnpm -F @offbeat/server artist:resolution -- backfill [--api-url URL]
  pnpm -F @offbeat/server artist:resolution -- list <festival-id> [--api-url URL]
  pnpm -F @offbeat/server artist:resolution -- retry <festival-id> [--api-url URL]
  pnpm -F @offbeat/server artist:resolution -- override <festival-id> <billing-key>
    --credit '<artist-id>|<credited-as>|<performer|presenter|guest>' [--credit ...]
    [--title 'Presented title'] [--qualifier dj_set|live|ambient_set|hybrid_set]
    [--api-url URL]

ADMIN_SECRET_KEY must contain the 64-character Ed25519 admin secret.`;

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function hexToBytes(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let index = 0; index < bytes.length; index += 1) {
		bytes[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
	}
	return bytes;
}

function option(args: string[], name: string): string | undefined {
	const index = args.indexOf(name);
	return index >= 0 ? args[index + 1] : undefined;
}

function repeatedOption(args: string[], name: string): string[] {
	const values: string[] = [];
	for (let index = 0; index < args.length; index += 1) {
		if (args[index] === name && args[index + 1]) values.push(args[index + 1]);
	}
	return values;
}

function usage(): never {
	throw new Error(USAGE);
}

function parseCommand(value: string | undefined): Command {
	if (value === "backfill" || value === "list" || value === "override" || value === "retry") {
		return value;
	}
	return usage();
}

function parseFestivalId(value: string | undefined): string {
	if (!value || !/^[a-zA-Z0-9_-]{1,200}$/.test(value)) return usage();
	return value;
}

function parseOverrideBody(args: string[]): string {
	const billingKey = args[2];
	if (!billingKey) return usage();
	const credits = repeatedOption(args, "--credit").map((value) => {
		const [artistId, creditedAs, role] = value.split("|");
		if (
			!artistId ||
			!creditedAs ||
			!(role === "performer" || role === "presenter" || role === "guest")
		) {
			throw new Error(`Invalid --credit value: ${value}`);
		}
		return { artistId, creditedAs, role };
	});
	if (credits.length === 0) throw new Error("At least one --credit is required");
	const performanceQualifiers = repeatedOption(args, "--qualifier");
	const presentedTitle = option(args, "--title");
	return JSON.stringify({
		billingKey,
		credits,
		...(presentedTitle ? { presentedTitle } : {}),
		...(performanceQualifiers.length > 0 ? { performanceQualifiers } : {}),
	});
}

function buildRequest(command: Command, args: string[]): RequestSpec {
	if (command === "backfill") {
		return { path: "/artist-resolutions/backfill", method: "POST" };
	}
	const festivalId = parseFestivalId(args[1]);
	const basePath = `/festivals/${festivalId}/artist-resolutions`;
	if (command === "list") return { path: basePath, method: "GET" };
	if (command === "retry") return { path: `${basePath}/retry`, method: "POST" };
	return { path: basePath, method: "PUT", body: parseOverrideBody(args) };
}

async function adminHeaders(
	secretHex: string,
	request: RequestSpec,
): Promise<Record<string, string>> {
	const secretKey = hexToBytes(secretHex);
	const timestamp = String(Math.floor(Date.now() / 1000));
	const nonce = crypto.randomUUID();
	const body = new TextEncoder().encode(request.body ?? "");
	const bodyHash = bytesToHex(new Uint8Array(await crypto.subtle.digest("SHA-256", body)));
	const message = `${request.method}\n${request.path}\n${timestamp}\n${nonce}\n${bodyHash}`;
	return {
		"Content-Type": "application/json",
		"X-Admin-Key": bytesToHex(ed25519.getPublicKey(secretKey)),
		"X-Admin-Sig": bytesToHex(ed25519.sign(new TextEncoder().encode(message), secretKey)),
		"X-Admin-Timestamp": timestamp,
		"X-Admin-Nonce": nonce,
	};
}

async function responseBody(response: Response): Promise<unknown> {
	const text = await response.text();
	if (!text) return null;
	try {
		return JSON.parse(text) as unknown;
	} catch {
		return text;
	}
}

async function run(args: string[]): Promise<void> {
	if (args.includes("--help") || args.includes("-h")) {
		process.stdout.write(`${USAGE}\n`);
		return;
	}
	const command = parseCommand(args[0]);
	const secretHex = process.env.ADMIN_SECRET_KEY;
	if (!secretHex || !/^[0-9a-f]{64}$/i.test(secretHex)) {
		throw new Error("ADMIN_SECRET_KEY must be a 64-character hex secret");
	}
	const request = buildRequest(command, args);
	const apiUrl = option(args, "--api-url") ?? "http://localhost:8787";
	const response = await fetch(new URL(request.path, apiUrl), {
		method: request.method,
		headers: await adminHeaders(secretHex, request),
		body: request.body,
	});
	const body = await responseBody(response);
	if (!response.ok) {
		throw new Error(`Artist resolution request failed (${response.status}): ${String(body)}`);
	}
	process.stdout.write(`${JSON.stringify(body, null, 2)}\n`);
}

async function main(): Promise<void> {
	try {
		await run(process.argv.slice(2));
	} catch (error) {
		process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
		process.exitCode = 1;
	}
}

void main();
