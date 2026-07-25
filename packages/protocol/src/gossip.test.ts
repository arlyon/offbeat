import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { describe, expect, it } from "vitest";
import {
	FestivalUpdateKind,
	GossipEnvelopeSchema,
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
});
