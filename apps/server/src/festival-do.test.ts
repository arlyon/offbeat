import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { ed25519 } from "@noble/curves/ed25519.js";
import {
	FestivalUpdateKind,
	GossipEnvelopeSchema,
	RelayClientMessageSchema,
	RelayServerMessageSchema,
} from "@offbeat/protocol";
import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import * as Y from "yjs";
import { festivalUpdateSigningPayload } from "./signing";

let worker: Unstable_DevWorker;
let workerUrl: string;

const FESTIVAL_ID = "fest-do-test";
const RELAY_ACK_CAPABILITY_TOPIC = "__offbeat/relay-ack/v1";

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
}

function hexToBytes(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let i = 0; i < hex.length; i += 2) {
		bytes[i / 2] = Number.parseInt(hex.substring(i, i + 2), 16);
	}
	return bytes;
}

function generateKeypair() {
	const { secretKey, publicKey } = ed25519.keygen();
	return { secretKey, publicKey, publicKeyHex: bytesToHex(publicKey) };
}

/** Register a user via WebAuthn dev bypass and return the attestation. */
async function registerUser(pubKeyHex: string) {
	const beginResp = await worker.fetch("/auth/register/begin", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ userId: `fest-do-test-${pubKeyHex.slice(0, 8)}` }),
	});
	const { challenge } = (await beginResp.json()) as { challenge: string };
	const completeResp = await worker.fetch("/auth/register/complete", {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({ webauthnResponse: {}, challenge, ed25519PublicKey: pubKeyHex }),
	});
	return (await completeResp.json()) as {
		attestation: { message: string; signature: string; issuer: string };
	};
}

/** Build protobuf auth message for WS authentication. */
function buildAuthMsg(
	attestation: { message: string; signature: string; issuer: string },
	kp: { secretKey: Uint8Array; publicKey: Uint8Array },
) {
	const timestamp = Math.floor(Date.now() / 1000).toString();
	const sessionMsg = new TextEncoder().encode(`session:${timestamp}`);
	const sessionSig = ed25519.sign(sessionMsg, kp.secretKey);
	return create(RelayClientMessageSchema, {
		msg: {
			case: "auth",
			value: {
				publicKey: kp.publicKey,
				attestation: {
					message: attestation.message,
					signature: hexToBytes(attestation.signature),
					issuer: hexToBytes(attestation.issuer),
				},
				signature: sessionSig,
				timestamp,
			},
		},
	});
}

/** Connect to Festival DO WS, return WebSocket + helpers. */
async function connectWS(festivalId: string) {
	const url = `${workerUrl}/festivals/${festivalId}/ws`;
	const ws = new WebSocket(url);
	await new Promise<void>((resolve, reject) => {
		ws.onopen = () => resolve();
		ws.onerror = (e) => reject(new Error(`WS failed: ${e}`));
		setTimeout(() => reject(new Error("WS timeout")), 5000);
	});
	// Drain hello message
	ws.binaryType = "arraybuffer";
	return ws;
}

function sendClientMsg(
	ws: WebSocket,
	msg: Parameters<typeof create<typeof RelayClientMessageSchema>>[1],
) {
	const m = create(RelayClientMessageSchema, msg);
	ws.send(toBinary(RelayClientMessageSchema, m));
}

/** Wait for a specific server message case. */
function waitForMsg(ws: WebSocket, expectedCase: string, timeoutMs = 5000) {
	return new Promise<ReturnType<typeof fromBinary<typeof RelayServerMessageSchema>>>((resolve, reject) => {
		const timeout = setTimeout(
			() => reject(new Error(`Timeout waiting for ${expectedCase}`)),
			timeoutMs,
		);
		const handler = (event: MessageEvent) => {
			const msg = fromBinary(
				RelayServerMessageSchema,
				new Uint8Array(event.data as ArrayBuffer),
			);
			if (msg.msg.case === expectedCase) {
				clearTimeout(timeout);
				ws.removeEventListener("message", handler);
				resolve(msg);
			}
		};
		ws.addEventListener("message", handler);
	});
}

/** Drain all pending messages (like hello on connect). */
async function drainMessages(_ws: WebSocket, ms = 100) {
	await new Promise((r) => setTimeout(r, ms));
}

beforeAll(async () => {
	worker = await unstable_dev("src/index.ts", {
		experimental: { disableExperimentalWarning: true },
	});
	workerUrl = `ws://${worker.address}:${worker.port}`;

	// Warmup
	for (let i = 0; i < 10; i++) {
		try {
			const resp = await worker.fetch("/festivals");
			if (resp.ok) break;
		} catch {
			await new Promise((r) => setTimeout(r, 500));
		}
	}

	// Configure the Festival DO
	await worker.fetch(`/festivals/${FESTIVAL_ID}/config`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			opensAt: "2020-01-01T00:00:00.000Z",
			closesAt: "2030-12-31T23:59:59.999Z",
			festivalId: FESTIVAL_ID,
			lat: 51.5,
			lon: -0.1,
		}),
	});
}, 60000);

afterAll(async () => {
	await worker.stop();
});

describe("FestivalDO lane split", () => {
	describe("schema migration", () => {
		it("GET /public-key returns a valid hex key", async () => {
			const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/public-key`);
			expect(resp.status).toBe(200);
			const key = await resp.text();
			expect(key).toMatch(/^[0-9a-f]{64}$/);
		});

		it("GET /config returns the configured values", async () => {
			const resp = await worker.fetch(`/festivals/${FESTIVAL_ID}/config`);
			expect(resp.status).toBe(200);
			const config = (await resp.json()) as { festivalId: string };
			expect(config.festivalId).toBe(FESTIVAL_ID);
		});
	});

	describe("WS protobuf messaging", () => {
		it("receives hello on connect", async () => {
			const ws = await connectWS(FESTIVAL_ID);
			const hello = await waitForMsg(ws, "hello", 2000);
			expect(hello.msg.case).toBe("hello");
			if (hello.msg.case === "hello") {
				expect(hello.msg.value.endpointId).toMatch(/^[0-9a-f]{64}$/);
			}
			ws.close();
		});

		it("subscribes to a topic", async () => {
			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);

			const subPromise = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, {
				msg: { case: "subscribe", value: { topics: [`festival/${FESTIVAL_ID}/chat`] } },
			});
			const sub = await subPromise;
			expect(sub.msg.case).toBe("subscribed");
			if (sub.msg.case === "subscribed") {
				expect(sub.msg.value.topics).toContain(`festival/${FESTIVAL_ID}/chat`);
			}
			ws.close();
		});

		it("rejects gossip from unauthenticated client", async () => {
			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);

			// Subscribe first
			const subPromise = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, {
				msg: { case: "subscribe", value: { topics: [`festival/${FESTIVAL_ID}/chat`] } },
			});
			await subPromise;

			// Try sending gossip without auth
			const errPromise = waitForMsg(ws, "error");
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "msg-1",
						userId: "user-1",
						displayName: "Test",
						text: "hello",
						topic: `festival/${FESTIVAL_ID}/chat`,
						timestamp: new Date().toISOString(),
					},
				},
			});
			sendClientMsg(ws, {
				msg: {
					case: "gossip",
					value: { topic: `festival/${FESTIVAL_ID}/chat`, message: envelope },
				},
			});
			const err = await errPromise;
			expect(err.msg.case).toBe("error");
			if (err.msg.case === "error") {
				expect(err.msg.value.error).toContain("Auth required");
			}
			ws.close();
		});

		it("rejects festivalUpdate from authenticated client", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);

			// Authenticate
			const authOkPromise = waitForMsg(ws, "authOk");
			const authMsg = buildAuthMsg(attestation, kp);
			ws.send(toBinary(RelayClientMessageSchema, authMsg));
			await authOkPromise;

			// Subscribe
			const subPromise = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, {
				msg: {
					case: "subscribe",
					value: { topics: [`festival/${FESTIVAL_ID}/state`] },
				},
			});
			await subPromise;

			// Try sending a festivalUpdate — should be rejected
			const errPromise = waitForMsg(ws, "error");
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "festivalUpdate",
					value: {
						docId: `festival/${FESTIVAL_ID}/state`,
						signedUpdate: {
							update: new Uint8Array([1, 2, 3]),
							author: "attacker",
							signature: new Uint8Array([4, 5, 6]),
						},
					},
				},
			});
			sendClientMsg(ws, {
				msg: {
					case: "gossip",
					value: { topic: `festival/${FESTIVAL_ID}/state`, message: envelope },
				},
			});
			const err = await errPromise;
			expect(err.msg.case).toBe("error");
			if (err.msg.case === "error") {
				expect(err.msg.value.error).toContain("Clients cannot send festival updates");
			}
			ws.close();
		});
	});

	describe("gossip routing", () => {
		it("routes chat messages to public_gossip_log and broadcasts", async () => {
			const kp1 = generateKeypair();
			const kp2 = generateKeypair();
			const { attestation: att1 } = await registerUser(kp1.publicKeyHex);
			const { attestation: att2 } = await registerUser(kp2.publicKeyHex);

			const topic = `festival/${FESTIVAL_ID}/chat/${crypto.randomUUID()}`;

			// Client A: connect, auth, subscribe
			const wsA = await connectWS(FESTIVAL_ID);
			await drainMessages(wsA);
			const authOkA = waitForMsg(wsA, "authOk");
			wsA.send(toBinary(RelayClientMessageSchema, buildAuthMsg(att1, kp1)));
			await authOkA;
			const subA = waitForMsg(wsA, "subscribed");
			sendClientMsg(wsA, {
				msg: {
					case: "subscribe",
					value: { topics: [topic, RELAY_ACK_CAPABILITY_TOPIC] },
				},
			});
			await subA;

			// Client B: connect, auth, subscribe
			const wsB = await connectWS(FESTIVAL_ID);
			await drainMessages(wsB);
			const authOkB = waitForMsg(wsB, "authOk");
			wsB.send(toBinary(RelayClientMessageSchema, buildAuthMsg(att2, kp2)));
			await authOkB;
			const subB = waitForMsg(wsB, "subscribed");
			sendClientMsg(wsB, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await subB;

			// Both peers listen. The sender echo is the positive persistence
			// acknowledgement used by the durable client outbox.
			const senderAckPromise = waitForMsg(wsA, "gossip");
			const broadcastPromise = waitForMsg(wsB, "gossip");

			// A sends a chat message
			const chatEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "chat-route-test",
						userId: kp1.publicKeyHex,
						displayName: "UserA",
						text: "routed chat",
						topic,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(wsA, {
				msg: { case: "gossip", value: { topic, message: chatEnvelope } },
			});

			const senderAck = await senderAckPromise;
			expect(senderAck.msg.case).toBe("gossip");

			// B receives the broadcast
			const broadcast = await broadcastPromise;
			expect(broadcast.msg.case).toBe("gossip");
			if (broadcast.msg.case === "gossip") {
				expect(broadcast.msg.value.topic).toBe(topic);
				if (senderAck.msg.case === "gossip") {
					expect(senderAck.msg.value.seq).toBe(broadcast.msg.value.seq);
				}
				const payload = broadcast.msg.value.message?.payload;
				expect(payload?.case).toBe("chat");
				if (payload?.case === "chat") {
					expect(payload.value.text).toBe("routed chat");
				}
			}

			// Retrying the exact envelope is idempotent and acknowledges the
			// original durable sequence rather than appending another row.
			const retryAckPromise = waitForMsg(wsA, "gossip");
			sendClientMsg(wsA, {
				msg: { case: "gossip", value: { topic, message: chatEnvelope } },
			});
			const retryAck = await retryAckPromise;
			expect(retryAck.msg.case).toBe("gossip");
			if (retryAck.msg.case === "gossip" && senderAck.msg.case === "gossip") {
				expect(retryAck.msg.value.seq).toBe(senderAck.msg.value.seq);
			}

			wsA.close();
			wsB.close();
		});

		it("routes encryptedChat to group_gossip_log", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);
			const groupTopic = "group/abc123/chat";

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const authOk = waitForMsg(ws, "authOk");
			ws.send(toBinary(RelayClientMessageSchema, buildAuthMsg(attestation, kp)));
			await authOk;
			// Advertise acknowledgement support without subscribing to the group.
			const ackSubscription = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, {
				msg: {
					case: "subscribe",
					value: { topics: [RELAY_ACK_CAPABILITY_TOPIC] },
				},
			});
			await ackSubscription;

			// Send encryptedChat without subscribing to the group. The sender must
			// still receive a persistence acknowledgement for durable outbox delivery.
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "encryptedChat",
					value: {
						encrypted: new Uint8Array([0xde, 0xad]),
						groupKeyId: "abc123",
					},
				},
			});
			const persistenceAck = waitForMsg(ws, "gossip");
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic: groupTopic, message: envelope } },
			});
			expect((await persistenceAck).msg.case).toBe("gossip");

			// Verify it's NOT in public catchup (should be in group table instead)
			const catchupPromise = waitForMsg(ws, "catchup");
			sendClientMsg(ws, {
				msg: { case: "catchup", value: { topic: groupTopic, sinceSeq: 0n } },
			});
			const catchup = await catchupPromise;
			expect(catchup.msg.case).toBe("catchup");
			if (catchup.msg.case === "catchup") {
				// Group catchup reads from group_gossip_log
				expect(catchup.msg.value.messages.length).toBeGreaterThanOrEqual(1);
			}

			ws.close();
		});
	});

	describe("svExchange with signed checkpoints", () => {
		it("returns an authority-signed checkpoint through the gossip path", async () => {
			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const docId = `festival/${FESTIVAL_ID}/state`;
			const sv = Y.encodeStateVector(new Y.Doc());
			const publicKeyHex = await (await worker.fetch(`/festivals/${FESTIVAL_ID}/public-key`)).text();

			const checkpointPromise = waitForMsg(ws, "gossip");
			sendClientMsg(ws, {
				msg: { case: "svExchange", value: { docId, sv } },
			});

			const response = await checkpointPromise;
			expect(response.msg.case).toBe("gossip");
			if (response.msg.case === "gossip") {
				const festival = response.msg.value.message?.payload;
				expect(festival?.case).toBe("festivalUpdate");
				if (festival?.case === "festivalUpdate") {
					expect(festival.value.docId).toBe(docId);
					expect(festival.value.kind).toBe(FestivalUpdateKind.CHECKPOINT);
					expect(festival.value.authoritySeq).toBeGreaterThan(0n);
					const signed = festival.value.signedUpdate;
					expect(signed).toBeDefined();
					if (signed) {
						const payload = festivalUpdateSigningPayload(
							docId,
							festival.value.kind,
							festival.value.authoritySeq,
							signed.update,
						);
						expect(ed25519.verify(signed.signature, payload, hexToBytes(publicKeyHex))).toBe(
							true,
						);
					}
				}
			}

			ws.close();
		});
	});

	describe("catchup routing by topic prefix", () => {
		it("festival/ topic catches up from public_gossip_log", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);
			const topic = `festival/${FESTIVAL_ID}/chat/catchup-test`;

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const authOk = waitForMsg(ws, "authOk");
			ws.send(toBinary(RelayClientMessageSchema, buildAuthMsg(attestation, kp)));
			await authOk;
			const sub = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await sub;

			// Send a chat message
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "catchup-test-1",
						userId: kp.publicKeyHex,
						displayName: "Tester",
						text: "catchup me",
						topic,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic, message: envelope } },
			});
			await new Promise((r) => setTimeout(r, 100));

			// Request catchup
			const catchupPromise = waitForMsg(ws, "catchup");
			sendClientMsg(ws, {
				msg: { case: "catchup", value: { topic, sinceSeq: 0n } },
			});
			const catchup = await catchupPromise;
			expect(catchup.msg.case).toBe("catchup");
			if (catchup.msg.case === "catchup") {
				expect(catchup.msg.value.topic).toBe(topic);
				expect(catchup.msg.value.messages.length).toBeGreaterThanOrEqual(1);
				// Verify the message content
				const entry = catchup.msg.value.messages[0];
				expect(entry.message?.payload.case).toBe("chat");
			}

			ws.close();
		});

		it("non-festival topic catches up from group_gossip_log", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);
			const groupTopic = "group/xyz789/chat";

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const authOk = waitForMsg(ws, "authOk");
			ws.send(toBinary(RelayClientMessageSchema, buildAuthMsg(attestation, kp)));
			await authOk;
			const sub = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, { msg: { case: "subscribe", value: { topics: [groupTopic] } } });
			await sub;

			// Send encrypted chat
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "encryptedChat",
					value: { encrypted: new Uint8Array([0xca, 0xfe]), groupKeyId: "xyz789" },
				},
			});
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic: groupTopic, message: envelope } },
			});
			await new Promise((r) => setTimeout(r, 100));

			// Catchup should read from group_gossip_log
			const catchupPromise = waitForMsg(ws, "catchup");
			sendClientMsg(ws, {
				msg: { case: "catchup", value: { topic: groupTopic, sinceSeq: 0n } },
			});
			const catchup = await catchupPromise;
			expect(catchup.msg.case).toBe("catchup");
			if (catchup.msg.case === "catchup") {
				expect(catchup.msg.value.messages.length).toBeGreaterThanOrEqual(1);
			}

			ws.close();
		});
	});

	describe("chatCatchup routing", () => {
		it("festival/ chatCatchup filters by writer SV", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);
			const topic = `festival/${FESTIVAL_ID}/chat/sv-test`;

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const authOk = waitForMsg(ws, "authOk");
			ws.send(toBinary(RelayClientMessageSchema, buildAuthMsg(attestation, kp)));
			await authOk;
			const sub = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await sub;

			// Send two chat messages with different writerSeqs
			for (const seq of [1n, 2n]) {
				const envelope = create(GossipEnvelopeSchema, {
					payload: {
						case: "chat",
						value: {
							id: `sv-test-${seq}`,
							userId: kp.publicKeyHex,
							displayName: "Tester",
							text: `msg seq ${seq}`,
							topic,
							timestamp: new Date().toISOString(),
							writerSeq: seq,
						},
					},
				});
				sendClientMsg(ws, {
					msg: { case: "gossip", value: { topic, message: envelope } },
				});
			}
			await new Promise((r) => setTimeout(r, 100));

			// chatCatchup with SV that already has seq 1 for this writer
			const diffPromise = waitForMsg(ws, "chatDiff");
			sendClientMsg(ws, {
				msg: {
					case: "chatCatchup",
					value: { topic, sv: { [kp.publicKeyHex]: 1n }, limit: 50 },
				},
			});
			const chatDiff = await diffPromise;
			expect(chatDiff.msg.case).toBe("chatDiff");
			if (chatDiff.msg.case === "chatDiff") {
				expect(chatDiff.msg.value.topic).toBe(topic);
				// Should only include seq > 1 (i.e., seq 2)
				const chatMsgs = chatDiff.msg.value.messages.filter(
					(m) => m.payload.case === "chat" && m.payload.value.userId === kp.publicKeyHex,
				);
				for (const m of chatMsgs) {
					if (m.payload.case === "chat") {
						expect(m.payload.value.writerSeq).toBeGreaterThan(1n);
					}
				}
			}

			ws.close();
		});

		it("group chatCatchup returns all (can't filter encrypted)", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);
			const groupTopic = "group/chatcatchup-test/chat";

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const authOk = waitForMsg(ws, "authOk");
			ws.send(toBinary(RelayClientMessageSchema, buildAuthMsg(attestation, kp)));
			await authOk;
			const sub = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, { msg: { case: "subscribe", value: { topics: [groupTopic] } } });
			await sub;

			// Send encrypted chat
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "encryptedChat",
					value: { encrypted: new Uint8Array([0xbe, 0xef]), groupKeyId: "chatcatchup-test" },
				},
			});
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic: groupTopic, message: envelope } },
			});
			await new Promise((r) => setTimeout(r, 100));

			// chatCatchup for group — should return all messages regardless of SV
			const diffPromise = waitForMsg(ws, "chatDiff");
			sendClientMsg(ws, {
				msg: { case: "chatCatchup", value: { topic: groupTopic, sv: {}, limit: 50 } },
			});
			const chatDiff = await diffPromise;
			expect(chatDiff.msg.case).toBe("chatDiff");
			if (chatDiff.msg.case === "chatDiff") {
				expect(chatDiff.msg.value.messages.length).toBeGreaterThanOrEqual(1);
			}

			ws.close();
		});
	});

	describe("cross-lane isolation", () => {
		it("public chat messages don't appear in group catchup and vice versa", async () => {
			const kp = generateKeypair();
			const { attestation } = await registerUser(kp.publicKeyHex);
			const publicTopic = `festival/${FESTIVAL_ID}/chat/isolation`;
			const groupTopic = "group/isolation-test/chat";

			const ws = await connectWS(FESTIVAL_ID);
			await drainMessages(ws);
			const authOk = waitForMsg(ws, "authOk");
			ws.send(toBinary(RelayClientMessageSchema, buildAuthMsg(attestation, kp)));
			await authOk;
			const sub = waitForMsg(ws, "subscribed");
			sendClientMsg(ws, {
				msg: { case: "subscribe", value: { topics: [publicTopic, groupTopic] } },
			});
			await sub;

			// Send public chat
			const publicEnv = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "iso-pub",
						userId: kp.publicKeyHex,
						displayName: "Tester",
						text: "public only",
						topic: publicTopic,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic: publicTopic, message: publicEnv } },
			});

			// Send group chat
			const groupEnv = create(GossipEnvelopeSchema, {
				payload: {
					case: "encryptedChat",
					value: { encrypted: new Uint8Array([0x01]), groupKeyId: "isolation-test" },
				},
			});
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic: groupTopic, message: groupEnv } },
			});
			await new Promise((r) => setTimeout(r, 150));

			// Catchup on public topic — should only have public chat
			const pubCatchup = waitForMsg(ws, "catchup");
			sendClientMsg(ws, {
				msg: { case: "catchup", value: { topic: publicTopic, sinceSeq: 0n } },
			});
			const pubResult = await pubCatchup;
			if (pubResult.msg.case === "catchup") {
				for (const entry of pubResult.msg.value.messages) {
					// No encrypted payloads should be in the public log
					expect(entry.message?.payload.case).not.toBe("encryptedChat");
				}
			}

			// Catchup on group topic — should only have encrypted chat
			const groupCatchup = waitForMsg(ws, "catchup");
			sendClientMsg(ws, {
				msg: { case: "catchup", value: { topic: groupTopic, sinceSeq: 0n } },
			});
			const groupResult = await groupCatchup;
			if (groupResult.msg.case === "catchup") {
				for (const entry of groupResult.msg.value.messages) {
					expect(entry.message?.payload.case).not.toBe("chat");
				}
			}

			ws.close();
		});
	});
});
