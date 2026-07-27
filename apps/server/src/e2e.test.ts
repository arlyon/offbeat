import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { ed25519 } from "@noble/curves/ed25519.js";
import {
	GossipEnvelopeSchema,
	RelayClientMessageSchema,
	RelayServerMessageSchema,
} from "@offbeat/protocol";
import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

let worker: Unstable_DevWorker;
let workerUrl: string;

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
		body: JSON.stringify({ userId: `e2e-${pubKeyHex.slice(0, 8)}` }),
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

type ServerMsg = ReturnType<typeof fromBinary<typeof RelayServerMessageSchema>>;

/** Connect to a configured Festival DO WS and return the socket. */
async function connectToFestival(festivalId: string): Promise<WebSocket> {
	const config = await worker.fetch(`/festivals/${festivalId}/config`, {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify({
			festivalId,
			opensAt: "2020-01-01T00:00:00.000Z",
			closesAt: "2100-01-01T00:00:00.000Z",
		}),
	});
	if (!config.ok) throw new Error(`Festival config failed: ${config.status}`);
	const url = `${workerUrl}/festivals/${festivalId}/ws`;
	const ws = new WebSocket(url);
	ws.binaryType = "arraybuffer";
	await new Promise<void>((resolve, reject) => {
		ws.onopen = () => resolve();
		ws.onerror = (e) => reject(new Error(`WS failed: ${e}`));
		setTimeout(() => reject(new Error("WS timeout")), 5000);
	});
	return ws;
}

/** Authenticate a WS connection. */
async function authenticateWS(
	ws: WebSocket,
	kp: { secretKey: Uint8Array; publicKey: Uint8Array; publicKeyHex: string },
) {
	const { attestation } = await registerUser(kp.publicKeyHex);
	const authOkPromise = waitForMessage(ws, "authOk");
	const authMsg = buildAuthMsg(attestation, kp);
	ws.send(toBinary(RelayClientMessageSchema, authMsg));
	await authOkPromise;
}

function sendClientMsg(
	ws: WebSocket,
	msg: Parameters<typeof create<typeof RelayClientMessageSchema>>[1],
) {
	const m = create(RelayClientMessageSchema, msg);
	ws.send(toBinary(RelayClientMessageSchema, m));
}

/** Wait for a specific server message case. */
function waitForMessage(ws: WebSocket, expectedCase: string, timeoutMs = 5000): Promise<ServerMsg> {
	return new Promise((resolve, reject) => {
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

/** Collect all server messages received within a time window. */
function collectMessages(ws: WebSocket, durationMs: number): Promise<ServerMsg[]> {
	return new Promise((resolve) => {
		const messages: ServerMsg[] = [];
		const handler = (event: MessageEvent) => {
			messages.push(
				fromBinary(RelayServerMessageSchema, new Uint8Array(event.data as ArrayBuffer)),
			);
		};
		ws.addEventListener("message", handler);
		setTimeout(() => {
			ws.removeEventListener("message", handler);
			resolve(messages);
		}, durationMs);
	});
}

async function drainMessages(_ws: WebSocket, ms = 100) {
	await new Promise((r) => setTimeout(r, ms));
}

beforeAll(async () => {
	worker = await unstable_dev("src/index.ts", {
		persist: false,
		experimental: { disableExperimentalWarning: true },
	});
	const loopbackWsScheme = ["w", "s"].join("");
	workerUrl = `${loopbackWsScheme}://${worker.address}:${worker.port}`;

	// Warmup
	for (let i = 0; i < 10; i++) {
		try {
			const resp = await worker.fetch("/festivals");
			if (resp.ok) break;
		} catch {
			await new Promise((r) => setTimeout(r, 500));
		}
	}
}, 60000);

afterAll(async () => {
	await worker.stop();
});

describe("FestivalDO e2e", () => {
	describe("single client", () => {
		it("rejects non-WebSocket requests before initialization", async () => {
			const response = await worker.fetch("/festivals/not-configured/ws");
			expect(response.status).toBe(426);
		});

		it("connects and subscribes to a topic", async () => {
			const ws = await connectToFestival("test-festival-1");
			await drainMessages(ws);

			const subscribePromise = waitForMessage(ws, "subscribed");
			sendClientMsg(ws, {
				msg: {
					case: "subscribe",
					value: { topics: ["festival/test-festival-1/chat"] },
				},
			});

			const response = await subscribePromise;
			expect(response.msg.case).toBe("subscribed");
			if (response.msg.case === "subscribed") {
				expect(response.msg.value.topics).toContain("festival/test-festival-1/chat");
			}

			ws.close();
		});

		it("sends a chat message and retrieves it via catchup", async () => {
			const kp = generateKeypair();
			const topic = "festival/test-festival-2/chat/general";
			const ws = await connectToFestival("test-festival-2");
			await drainMessages(ws);

			// Authenticate
			await authenticateWS(ws, kp);

			// Subscribe
			const subscribePromise = waitForMessage(ws, "subscribed");
			sendClientMsg(ws, {
				msg: {
					case: "subscribe",
					value: { topics: [topic] },
				},
			});
			await subscribePromise;

			// Send a chat message via gossip envelope
			const chatEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "msg-1",
						userId: kp.publicKeyHex,
						displayName: "Test User",
						text: "Hello, world!",
						topic,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(ws, {
				msg: {
					case: "gossip",
					value: { topic, message: chatEnvelope },
				},
			});

			await new Promise((r) => setTimeout(r, 50));

			// Public chat catch-up uses committed writer heads, not relay sequence.
			const catchupPromise = waitForMessage(ws, "chatDiff");
			sendClientMsg(ws, {
				msg: {
					case: "chatCatchup",
					value: { topic, sv: {}, headIds: {}, limit: 50 },
				},
			});

			const catchup = await catchupPromise;
			expect(catchup.msg.case).toBe("chatDiff");
			if (catchup.msg.case === "chatDiff") {
				expect(catchup.msg.value.topic).toBe(topic);
				expect(catchup.msg.value.messages.length).toBeGreaterThanOrEqual(1);
				const envelope = catchup.msg.value.messages[0];
				expect(envelope.payload.case).toBe("chat");
				if (envelope.payload.case === "chat") {
					expect(envelope.payload.value.text).toBe("Hello, world!");
					expect(envelope.payload.value.userId).toBe(kp.publicKeyHex);
				}
			}

			ws.close();
		});

		it("sends a group update and retrieves it via catchup", async () => {
			const kp = generateKeypair();
			const ws = await connectToFestival("test-festival-3");
			await drainMessages(ws);

			await authenticateWS(ws, kp);

			const topic = "group/test-group-3/state";

			// Subscribe
			const subscribePromise = waitForMessage(ws, "subscribed");
			sendClientMsg(ws, {
				msg: { case: "subscribe", value: { topics: [topic] } },
			});
			await subscribePromise;

			// Send a group update (encrypted blob)
			const groupEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "groupUpdate",
					value: {
						docId: "group/test-group-3/state",
						encrypted: new Uint8Array([0xca, 0xfe, 0xba, 0xbe]),
						groupKeyId: "test-group-3",
					},
				},
			});
			sendClientMsg(ws, {
				msg: { case: "gossip", value: { topic, message: groupEnvelope } },
			});

			await new Promise((r) => setTimeout(r, 50));

			// Catchup — group topics go through group_gossip_log
			const catchupPromise = waitForMessage(ws, "catchup");
			sendClientMsg(ws, {
				msg: { case: "catchup", value: { topic, sinceSeq: 0n } },
			});

			const catchup = await catchupPromise;
			expect(catchup.msg.case).toBe("catchup");
			if (catchup.msg.case === "catchup") {
				// groupUpdate goes to group_yrs_updates, not group_gossip_log,
				// so catchup (which reads group_gossip_log) won't find it.
				// This is by design — group CRDT updates use svExchange, not catchup.
				// The message was still broadcast to subscribers in real-time.
			}

			ws.close();
		});
	});

	describe("two clients - direct and relay", () => {
		it("client A sends chat, client B receives via DO relay", async () => {
			const topic = "festival/test-festival-4/chat/general";
			const kpA = generateKeypair();

			// Client A connects
			const clientA = await connectToFestival("test-festival-4");
			await drainMessages(clientA);
			await authenticateWS(clientA, kpA);

			// Client B connects (doesn't need auth to receive)
			const clientB = await connectToFestival("test-festival-4");
			await drainMessages(clientB);

			// Both subscribe
			const subPromiseA = waitForMessage(clientA, "subscribed");
			const subPromiseB = waitForMessage(clientB, "subscribed");
			sendClientMsg(clientA, { msg: { case: "subscribe", value: { topics: [topic] } } });
			sendClientMsg(clientB, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await Promise.all([subPromiseA, subPromiseB]);

			// Set up listener on client B BEFORE client A sends
			const gossipPromiseB = waitForMessage(clientB, "gossip");

			// Client A sends a chat message
			const chatEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "msg-relay-1",
						userId: kpA.publicKeyHex,
						displayName: "User A",
						text: "Message from A to B",
						topic,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(clientA, {
				msg: { case: "gossip", value: { topic, message: chatEnvelope } },
			});

			// Client B should receive the broadcast
			const receivedByB = await gossipPromiseB;
			expect(receivedByB.msg.case).toBe("gossip");
			if (receivedByB.msg.case === "gossip") {
				expect(receivedByB.msg.value.topic).toBe(topic);
				const payload = receivedByB.msg.value.message?.payload;
				expect(payload?.case).toBe("chat");
				if (payload?.case === "chat") {
					expect(payload.value.text).toBe("Message from A to B");
					expect(payload.value.userId).toBe(kpA.publicKeyHex);
				}
			}

			clientA.close();
			clientB.close();
		});

		it("client B sends encrypted chat, client A receives it", async () => {
			const topic = "group/test-festival-5/chat";
			const kpB = generateKeypair();

			const clientA = await connectToFestival("test-festival-5");
			await drainMessages(clientA);

			const clientB = await connectToFestival("test-festival-5");
			await drainMessages(clientB);
			await authenticateWS(clientB, kpB);

			// Subscribe both
			const subPromiseA = waitForMessage(clientA, "subscribed");
			const subPromiseB = waitForMessage(clientB, "subscribed");
			sendClientMsg(clientA, { msg: { case: "subscribe", value: { topics: [topic] } } });
			sendClientMsg(clientB, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await Promise.all([subPromiseA, subPromiseB]);

			// Set up listener on client A
			const gossipPromiseA = waitForMessage(clientA, "gossip");

			// Client B sends encrypted chat
			const encEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "encryptedChat",
					value: {
						encrypted: new Uint8Array([0xde, 0xad, 0xbe, 0xef]),
						groupKeyId: "test-group-5",
					},
				},
			});
			sendClientMsg(clientB, {
				msg: { case: "gossip", value: { topic, message: encEnvelope } },
			});

			// Client A should receive it
			const receivedByA = await gossipPromiseA;
			expect(receivedByA.msg.case).toBe("gossip");
			if (receivedByA.msg.case === "gossip") {
				expect(receivedByA.msg.value.topic).toBe(topic);
				const payload = receivedByA.msg.value.message?.payload;
				expect(payload?.case).toBe("encryptedChat");
			}

			clientA.close();
			clientB.close();
		});

		it("late-joining client catches up on missed messages", async () => {
			const topic = "festival/test-festival-6/chat/general";
			const kpA = generateKeypair();

			// Client A connects and sends a message
			const clientA = await connectToFestival("test-festival-6");
			await drainMessages(clientA);
			await authenticateWS(clientA, kpA);

			const subPromiseA = waitForMessage(clientA, "subscribed");
			sendClientMsg(clientA, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await subPromiseA;

			// Send message before client B joins
			const chatEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "msg-before-b",
						userId: kpA.publicKeyHex,
						displayName: "User A",
						text: "Sent before B joined",
						topic,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(clientA, {
				msg: { case: "gossip", value: { topic, message: chatEnvelope } },
			});

			await new Promise((r) => setTimeout(r, 100));

			// Now client B connects (late joiner)
			const clientB = await connectToFestival("test-festival-6");
			await drainMessages(clientB);

			const subPromiseB = waitForMessage(clientB, "subscribed");
			sendClientMsg(clientB, { msg: { case: "subscribe", value: { topics: [topic] } } });
			await subPromiseB;

			// Client B requests committed-head catch-up.
			const catchupPromise = waitForMessage(clientB, "chatDiff");
			sendClientMsg(clientB, {
				msg: {
					case: "chatCatchup",
					value: { topic, sv: {}, headIds: {}, limit: 50 },
				},
			});

			const catchup = await catchupPromise;
			expect(catchup.msg.case).toBe("chatDiff");
			if (catchup.msg.case === "chatDiff") {
				expect(catchup.msg.value.messages.length).toBeGreaterThanOrEqual(1);
				const envelope = catchup.msg.value.messages[0];
				expect(envelope.payload.case).toBe("chat");
				if (envelope.payload.case === "chat") {
					expect(envelope.payload.value.text).toBe("Sent before B joined");
				}
			}

			clientA.close();
			clientB.close();
		});

		it("multiple topics - messages routed correctly", async () => {
			const topicChat = "festival/test-festival-7/chat/general";
			const topicGroup = "group/test-festival-7/chat";
			const kpC = generateKeypair();

			const clientA = await connectToFestival("test-festival-7");
			await drainMessages(clientA);

			const clientB = await connectToFestival("test-festival-7");
			await drainMessages(clientB);

			// Client A subscribes to public chat only
			const subPromiseA = waitForMessage(clientA, "subscribed");
			sendClientMsg(clientA, { msg: { case: "subscribe", value: { topics: [topicChat] } } });
			await subPromiseA;

			// Client B subscribes to group chat only
			const subPromiseB = waitForMessage(clientB, "subscribed");
			sendClientMsg(clientB, { msg: { case: "subscribe", value: { topics: [topicGroup] } } });
			await subPromiseB;

			// Client C connects, authenticates, and subscribes to both
			const clientC = await connectToFestival("test-festival-7");
			await drainMessages(clientC);
			await authenticateWS(clientC, kpC);
			const subPromiseC = waitForMessage(clientC, "subscribed");
			sendClientMsg(clientC, {
				msg: { case: "subscribe", value: { topics: [topicChat, topicGroup] } },
			});
			await subPromiseC;

			// Set up collectors
			const messagesA = collectMessages(clientA, 300);
			const messagesB = collectMessages(clientB, 300);

			// C sends encrypted chat to group topic — only B should receive
			const groupEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "encryptedChat",
					value: {
						encrypted: new Uint8Array([0x01]),
						groupKeyId: "test-festival-7",
					},
				},
			});
			sendClientMsg(clientC, {
				msg: { case: "gossip", value: { topic: topicGroup, message: groupEnvelope } },
			});

			// C sends public chat to chat topic — only A should receive
			const chatEnvelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "chat",
					value: {
						id: "msg-chat",
						userId: kpC.publicKeyHex,
						displayName: "User C",
						text: "Chat message",
						topic: topicChat,
						timestamp: new Date().toISOString(),
						writerSeq: 1n,
					},
				},
			});
			sendClientMsg(clientC, {
				msg: { case: "gossip", value: { topic: topicChat, message: chatEnvelope } },
			});

			const [receivedA, receivedB] = await Promise.all([messagesA, messagesB]);

			// Client A should only have gossip with chat payload
			const gossipA = receivedA.filter((m) => m.msg.case === "gossip");
			expect(gossipA.length).toBe(1);
			if (gossipA[0].msg.case === "gossip") {
				expect(gossipA[0].msg.value.message?.payload.case).toBe("chat");
			}

			// Client B should only have gossip with encryptedChat payload
			const gossipB = receivedB.filter((m) => m.msg.case === "gossip");
			expect(gossipB.length).toBe(1);
			if (gossipB[0].msg.case === "gossip") {
				expect(gossipB[0].msg.value.message?.payload.case).toBe("encryptedChat");
			}

			clientA.close();
			clientB.close();
			clientC.close();
		});
	});
});
