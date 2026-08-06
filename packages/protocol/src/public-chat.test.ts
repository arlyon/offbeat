import { create } from "@bufbuild/protobuf";
import { describe, expect, it } from "vitest";
import { ChatMessageSchema, publicChatSigningPayload, verifyPublicChatMessage } from "./index.js";

const WRITER_KEY_HEX = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";
const SIGNATURE_HEX =
	"6a9cdb6087a466b25b45df94e7fb45ab6804295b709c0ea0c77ea17178be6d1a08e5e612f61518c546b0d436a2d5abd06727e20f4eab561eb8de074a72222c0e";
const PAYLOAD_HEX =
	"6f6666626561742f7075626c69632d636861742f763100000000096d6573736167652d31000000106561346136633633653239633532306100000005416c696365000000164d6565742062792074686520736f756e64206465736b00000021666573746976616c2f6669656c646461792f636861742f6d61696e2d7374616765010000000a6d61696e2d737461676500000014323032362d30362d31345432303a30303a30305a0000000000000007000000000000002a00000020ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";

function hexToBytes(value: string): Uint8Array {
	return Uint8Array.from(value.match(/.{2}/g) ?? [], (byte) => Number.parseInt(byte, 16));
}

function bytesToHex(value: Uint8Array): string {
	return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

function signedFixture() {
	return create(ChatMessageSchema, {
		id: "message-1",
		userId: "ea4a6c63e29c520a",
		displayName: "Alice",
		text: "Meet by the sound desk",
		topic: "festival/fieldday/chat/main-stage",
		stageId: "main-stage",
		timestamp: "2026-06-14T20:00:00Z",
		writerSeq: 7n,
		logicalTime: 42n,
		writerKey: hexToBytes(WRITER_KEY_HEX),
		signature: hexToBytes(SIGNATURE_HEX),
	});
}

describe("public chat signatures", () => {
	it("matches the Rust canonical vector and verifies entirely offline", async () => {
		const message = signedFixture();
		expect(bytesToHex(publicChatSigningPayload(message))).toBe(PAYLOAD_HEX);
		expect(await verifyPublicChatMessage(message)).toBe(true);
	});

	it("binds content, topic, stage, and append-log position", async () => {
		const original = signedFixture();
		for (const changed of [
			create(ChatMessageSchema, { ...original, text: "altered" }),
			create(ChatMessageSchema, { ...original, topic: "festival/other/chat/main-stage" }),
			create(ChatMessageSchema, { ...original, stageId: "other-stage" }),
			create(ChatMessageSchema, { ...original, writerSeq: 8n }),
			create(ChatMessageSchema, { ...original, logicalTime: 43n }),
		]) {
			expect(await verifyPublicChatMessage(changed)).toBe(false);
		}
	});

	it("rejects a writer key that does not match the user ID", () => {
		const message = signedFixture();
		message.userId = "0000000000000000";
		expect(() => publicChatSigningPayload(message)).toThrow(/user ID/);
	});
});
