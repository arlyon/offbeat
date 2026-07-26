import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { describe, expect, it } from "vitest";
import {
	ChatMessageSchema,
	FestivalUpdateKind,
	GossipEnvelopeSchema,
	RelayClientMessageSchema,
	SignedUpdateSchema,
} from "./index.js";

describe("festival update protocol", () => {
	it("round-trips signed checkpoint context", () => {
		const envelope = create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId: "festival/fieldday2026/state",
					kind: FestivalUpdateKind.CHECKPOINT,
					authoritySeq: 42n,
					signedUpdate: create(SignedUpdateSchema, {
						update: Uint8Array.from([1, 2, 3]),
						author: "festival-do",
						signature: new Uint8Array(64).fill(7),
					}),
				},
			},
		});

		const decoded = fromBinary(GossipEnvelopeSchema, toBinary(GossipEnvelopeSchema, envelope));
		expect(decoded.payload.case).toBe("festivalUpdate");
		if (decoded.payload.case !== "festivalUpdate") throw new Error("wrong payload case");
		expect(decoded.payload.value.docId).toBe("festival/fieldday2026/state");
		expect(decoded.payload.value.kind).toBe(FestivalUpdateKind.CHECKPOINT);
		expect(decoded.payload.value.authoritySeq).toBe(42n);
		expect(decoded.payload.value.signedUpdate?.signature).toHaveLength(64);
	});

	it("round-trips Lamport order and writer head commitments", () => {
		const chat = create(ChatMessageSchema, {
			id: "alice-7",
			userId: "alice",
			text: "hello",
			topic: "festival/f/chat/general",
			writerSeq: 7n,
			logicalTime: 42n,
		});
		const decodedChat = fromBinary(ChatMessageSchema, toBinary(ChatMessageSchema, chat));
		expect(decodedChat.logicalTime).toBe(42n);

		const request = create(RelayClientMessageSchema, {
			msg: {
				case: "chatCatchup",
				value: {
					topic: chat.topic,
					sv: { alice: 7n },
					headIds: { alice: chat.id },
					limit: 50,
				},
			},
		});
		const decodedRequest = fromBinary(
			RelayClientMessageSchema,
			toBinary(RelayClientMessageSchema, request),
		);
		expect(decodedRequest.msg.case).toBe("chatCatchup");
		if (decodedRequest.msg.case !== "chatCatchup") throw new Error("wrong request case");
		expect(decodedRequest.msg.value.headIds.alice).toBe("alice-7");
	});
});
