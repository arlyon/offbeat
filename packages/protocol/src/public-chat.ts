import type { ChatMessage } from "./generated/offbeat/v1/types_pb.js";

const PUBLIC_CHAT_DOMAIN = new TextEncoder().encode("offbeat/public-chat/v1\0");
const textEncoder = new TextEncoder();

function bytesToHex(value: Uint8Array): string {
	return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function appendLengthPrefixed(parts: Uint8Array[], value: Uint8Array, field: string): void {
	if (value.byteLength > 0xffff_ffff) {
		throw new Error(`public chat ${field} is too large`);
	}
	const length = new Uint8Array(4);
	new DataView(length.buffer).setUint32(0, value.byteLength, false);
	parts.push(length, value);
}

function appendString(parts: Uint8Array[], value: string, field: string): void {
	appendLengthPrefixed(parts, textEncoder.encode(value), field);
}

function u64Bytes(value: bigint, field: string): Uint8Array {
	if (value <= 0n || value > 0xffff_ffff_ffff_ffffn) {
		throw new Error(`public chat ${field} is outside the uint64 range`);
	}
	const encoded = new Uint8Array(8);
	new DataView(encoded.buffer).setBigUint64(0, value, false);
	return encoded;
}

function publicChatChannel(message: ChatMessage): string {
	const parts = message.topic.split("/");
	if (
		parts.length !== 4 ||
		parts[0] !== "festival" ||
		parts[1].length === 0 ||
		parts[2] !== "chat" ||
		parts[3].length === 0
	) {
		throw new Error("invalid public chat topic");
	}
	return parts[3];
}

/** Canonical bytes signed by public-chat authors on every transport. */
export function publicChatSigningPayload(message: ChatMessage): Uint8Array {
	const channel = publicChatChannel(message);
	if (message.writerKey.byteLength !== 32) {
		throw new Error("public chat writer key must be 32 bytes");
	}
	if (message.stageId !== undefined ? message.stageId !== channel : channel !== "campsite") {
		throw new Error("public chat stage does not match its topic");
	}
	if (message.userId !== bytesToHex(message.writerKey.subarray(0, 8))) {
		throw new Error("public chat user ID does not match its writer key");
	}

	const parts: Uint8Array[] = [PUBLIC_CHAT_DOMAIN];
	appendString(parts, message.id, "message ID");
	appendString(parts, message.userId, "user ID");
	appendString(parts, message.displayName, "display name");
	appendString(parts, message.text, "text");
	appendString(parts, message.topic, "topic");
	if (message.stageId === undefined) {
		parts.push(Uint8Array.of(0));
	} else {
		parts.push(Uint8Array.of(1));
		appendString(parts, message.stageId, "stage ID");
	}
	appendString(parts, message.timestamp, "timestamp");
	parts.push(u64Bytes(message.writerSeq, "writer sequence"));
	parts.push(u64Bytes(message.logicalTime, "Lamport time"));
	appendLengthPrefixed(parts, message.writerKey, "writer key");

	const length = parts.reduce((total, part) => total + part.byteLength, 0);
	const payload = new Uint8Array(length);
	let offset = 0;
	for (const part of parts) {
		payload.set(part, offset);
		offset += part.byteLength;
	}
	return payload;
}

/** Verify authorship without contacting MainDO or FestivalDO. */
export async function verifyPublicChatMessage(message: ChatMessage): Promise<boolean> {
	if (message.signature.byteLength !== 64) return false;
	try {
		const key = await crypto.subtle.importKey(
			"raw",
			new Uint8Array(message.writerKey).buffer,
			{ name: "Ed25519" },
			false,
			["verify"],
		);
		return await crypto.subtle.verify(
			{ name: "Ed25519" },
			key,
			new Uint8Array(message.signature).buffer,
			new Uint8Array(publicChatSigningPayload(message)).buffer,
		);
	} catch {
		return false;
	}
}
