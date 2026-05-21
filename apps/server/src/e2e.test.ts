import { type Unstable_DevWorker, unstable_dev } from "wrangler";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

let worker: Unstable_DevWorker;
let workerUrl: string;

beforeAll(async () => {
	worker = await unstable_dev("src/index.ts", {
		experimental: { disableExperimentalWarning: true },
	});
	// worker.address is a string like "127.0.0.1" and worker.port is a number
	workerUrl = `ws://${worker.address}:${worker.port}`;

	// Warmup: make a simple HTTP request to ensure the worker is ready
	let ready = false;
	for (let i = 0; i < 10 && !ready; i++) {
		try {
			const resp = await worker.fetch("/festivals");
			if (resp.ok) {
				ready = true;
			}
		} catch {
			await new Promise((r) => setTimeout(r, 500));
		}
	}
});

afterAll(async () => {
	await worker.stop();
});

/**
 * Helper to create a WebSocket connection to the FestivalDO
 */
function connectToFestival(festivalId: string): Promise<WebSocket> {
	return new Promise((resolve, reject) => {
		const url = `${workerUrl}/festivals/${festivalId}/ws`;
		const ws = new WebSocket(url);

		ws.onopen = () => resolve(ws);
		ws.onerror = (e) => reject(new Error(`WebSocket connection failed: ${e}`));

		setTimeout(() => reject(new Error("WebSocket connection timeout")), 5000);
	});
}

/**
 * Helper to wait for a specific message type
 */
function waitForMessage(
	ws: WebSocket,
	expectedType: string,
	timeoutMs = 5000,
): Promise<Record<string, unknown>> {
	return new Promise((resolve, reject) => {
		const timeout = setTimeout(() => {
			reject(new Error(`Timeout waiting for message type: ${expectedType}`));
		}, timeoutMs);

		const handler = (event: MessageEvent) => {
			const data = JSON.parse(event.data as string);
			if (data.type === expectedType) {
				clearTimeout(timeout);
				ws.removeEventListener("message", handler);
				resolve(data);
			}
		};

		ws.addEventListener("message", handler);
	});
}

/**
 * Helper to collect all messages received within a time window
 */
function collectMessages(ws: WebSocket, durationMs: number): Promise<Record<string, unknown>[]> {
	return new Promise((resolve) => {
		const messages: Record<string, unknown>[] = [];

		const handler = (event: MessageEvent) => {
			messages.push(JSON.parse(event.data as string));
		};

		ws.addEventListener("message", handler);

		setTimeout(() => {
			ws.removeEventListener("message", handler);
			resolve(messages);
		}, durationMs);
	});
}

describe("FestivalDO e2e", () => {
	describe("single client", () => {
		it("connects and subscribes to a topic", async () => {
			const ws = await connectToFestival("test-festival-1");

			const subscribePromise = waitForMessage(ws, "subscribed");
			ws.send(
				JSON.stringify({
					type: "subscribe",
					topics: ["festival/test-festival-1/chat"],
				}),
			);

			const response = await subscribePromise;
			expect(response.type).toBe("subscribed");
			expect(response.topics).toContain("festival/test-festival-1/chat");

			ws.close();
		});

		it("sends a chat message and retrieves it via catchup", async () => {
			const ws = await connectToFestival("test-festival-2");

			// Subscribe first
			const subscribePromise = waitForMessage(ws, "subscribed");
			ws.send(
				JSON.stringify({
					type: "subscribe",
					topics: ["festival/test-festival-2/chat"],
				}),
			);
			await subscribePromise;

			// Send a chat message
			const chatMessage = {
				id: "msg-1",
				userId: "user-1",
				displayName: "Test User",
				text: "Hello, world!",
				topic: "festival/test-festival-2/chat",
				timestamp: new Date().toISOString(),
			};

			ws.send(
				JSON.stringify({
					type: "chat",
					topic: "festival/test-festival-2/chat",
					message: chatMessage,
				}),
			);

			// Small delay to ensure message is stored
			await new Promise((r) => setTimeout(r, 50));

			// Request catchup from seq 0
			const catchupPromise = waitForMessage(ws, "catchup");
			ws.send(
				JSON.stringify({
					type: "catchup",
					topic: "festival/test-festival-2/chat",
					sinceSeq: 0,
				}),
			);

			const catchup = await catchupPromise;
			expect(catchup.type).toBe("catchup");
			expect(catchup.topic).toBe("festival/test-festival-2/chat");
			expect(Array.isArray(catchup.chat)).toBe(true);
			expect((catchup.chat as unknown[]).length).toBeGreaterThanOrEqual(1);

			const storedMsg = (catchup.chat as { message: typeof chatMessage }[])[0].message;
			expect(storedMsg.text).toBe("Hello, world!");
			expect(storedMsg.userId).toBe("user-1");

			ws.close();
		});

		it("sends a relay message and retrieves it via catchup", async () => {
			const ws = await connectToFestival("test-festival-3");

			// Subscribe
			const subscribePromise = waitForMessage(ws, "subscribed");
			ws.send(
				JSON.stringify({
					type: "subscribe",
					topics: ["festival/test-festival-3/state"],
				}),
			);
			await subscribePromise;

			// Send a relay message (simulating an encrypted update)
			const relayData = btoa("encrypted-update-payload");

			ws.send(
				JSON.stringify({
					type: "relay",
					topic: "festival/test-festival-3/state",
					data: relayData,
				}),
			);

			// Small delay
			await new Promise((r) => setTimeout(r, 50));

			// Catchup
			const catchupPromise = waitForMessage(ws, "catchup");
			ws.send(
				JSON.stringify({
					type: "catchup",
					topic: "festival/test-festival-3/state",
					sinceSeq: 0,
				}),
			);

			const catchup = await catchupPromise;
			expect(catchup.type).toBe("catchup");
			expect(Array.isArray(catchup.relay)).toBe(true);
			expect((catchup.relay as unknown[]).length).toBeGreaterThanOrEqual(1);
			expect((catchup.relay as { data: string }[])[0].data).toBe(relayData);

			ws.close();
		});
	});

	describe("two clients - direct and relay", () => {
		it("client A sends chat, client B receives via DO relay", async () => {
			const topic = "festival/test-festival-4/chat";

			// Client A connects (direct)
			const clientA = await connectToFestival("test-festival-4");

			// Client B connects (will receive relayed messages)
			const clientB = await connectToFestival("test-festival-4");

			// Both subscribe to the same topic
			const subPromiseA = waitForMessage(clientA, "subscribed");
			const subPromiseB = waitForMessage(clientB, "subscribed");

			clientA.send(JSON.stringify({ type: "subscribe", topics: [topic] }));
			clientB.send(JSON.stringify({ type: "subscribe", topics: [topic] }));

			await Promise.all([subPromiseA, subPromiseB]);

			// Set up listener on client B BEFORE client A sends
			const chatPromiseB = waitForMessage(clientB, "chat");

			// Client A sends a chat message
			const chatMessage = {
				id: "msg-relay-1",
				userId: "user-a",
				displayName: "User A",
				text: "Message from A to B",
				topic,
				timestamp: new Date().toISOString(),
			};

			clientA.send(
				JSON.stringify({
					type: "chat",
					topic,
					message: chatMessage,
				}),
			);

			// Client B should receive the message
			const receivedByB = await chatPromiseB;
			expect(receivedByB.type).toBe("chat");
			expect(receivedByB.topic).toBe(topic);
			expect((receivedByB.message as { text: string }).text).toBe("Message from A to B");
			expect((receivedByB.message as { userId: string }).userId).toBe("user-a");
			expect(receivedByB.seq).toBeDefined();

			clientA.close();
			clientB.close();
		});

		it("client B sends relay update, client A receives it", async () => {
			const topic = "festival/test-festival-5/state";

			const clientA = await connectToFestival("test-festival-5");
			const clientB = await connectToFestival("test-festival-5");

			// Subscribe both
			const subPromiseA = waitForMessage(clientA, "subscribed");
			const subPromiseB = waitForMessage(clientB, "subscribed");

			clientA.send(JSON.stringify({ type: "subscribe", topics: [topic] }));
			clientB.send(JSON.stringify({ type: "subscribe", topics: [topic] }));

			await Promise.all([subPromiseA, subPromiseB]);

			// Set up listener on client A
			const relayPromiseA = waitForMessage(clientA, "relay");

			// Client B sends a relay message
			const relayData = btoa("crdt-update-from-b");

			clientB.send(
				JSON.stringify({
					type: "relay",
					topic,
					data: relayData,
				}),
			);

			// Client A should receive it
			const receivedByA = await relayPromiseA;
			expect(receivedByA.type).toBe("relay");
			expect(receivedByA.topic).toBe(topic);
			expect(receivedByA.data).toBe(relayData);
			expect(receivedByA.seq).toBeDefined();

			clientA.close();
			clientB.close();
		});

		it("late-joining client catches up on missed messages", async () => {
			const topic = "festival/test-festival-6/chat";

			// Client A connects and sends a message
			const clientA = await connectToFestival("test-festival-6");

			const subPromiseA = waitForMessage(clientA, "subscribed");
			clientA.send(JSON.stringify({ type: "subscribe", topics: [topic] }));
			await subPromiseA;

			// Send message before client B joins
			clientA.send(
				JSON.stringify({
					type: "chat",
					topic,
					message: {
						id: "msg-before-b",
						userId: "user-a",
						displayName: "User A",
						text: "Sent before B joined",
						topic,
						timestamp: new Date().toISOString(),
					},
				}),
			);

			// Small delay
			await new Promise((r) => setTimeout(r, 50));

			// Now client B connects (late joiner)
			const clientB = await connectToFestival("test-festival-6");

			const subPromiseB = waitForMessage(clientB, "subscribed");
			clientB.send(JSON.stringify({ type: "subscribe", topics: [topic] }));
			await subPromiseB;

			// Client B requests catchup
			const catchupPromise = waitForMessage(clientB, "catchup");
			clientB.send(
				JSON.stringify({
					type: "catchup",
					topic,
					sinceSeq: 0,
				}),
			);

			const catchup = await catchupPromise;
			expect(catchup.type).toBe("catchup");
			expect((catchup.chat as unknown[]).length).toBeGreaterThanOrEqual(1);
			expect((catchup.chat as { message: { text: string } }[])[0].message.text).toBe(
				"Sent before B joined",
			);

			clientA.close();
			clientB.close();
		});

		it("multiple topics - messages routed correctly", async () => {
			const topicChat = "festival/test-festival-7/chat";
			const topicState = "festival/test-festival-7/state";

			const clientA = await connectToFestival("test-festival-7");
			const clientB = await connectToFestival("test-festival-7");

			// Client A subscribes to chat only
			const subPromiseA = waitForMessage(clientA, "subscribed");
			clientA.send(JSON.stringify({ type: "subscribe", topics: [topicChat] }));
			await subPromiseA;

			// Client B subscribes to state only
			const subPromiseB = waitForMessage(clientB, "subscribed");
			clientB.send(JSON.stringify({ type: "subscribe", topics: [topicState] }));
			await subPromiseB;

			// Set up collectors for both clients
			const messagesA = collectMessages(clientA, 200);
			const messagesB = collectMessages(clientB, 200);

			// Send message to state topic from a third connection
			const clientC = await connectToFestival("test-festival-7");
			const subPromiseC = waitForMessage(clientC, "subscribed");
			clientC.send(JSON.stringify({ type: "subscribe", topics: [topicChat, topicState] }));
			await subPromiseC;

			// C sends to state topic - only B should receive
			clientC.send(
				JSON.stringify({
					type: "relay",
					topic: topicState,
					data: btoa("state-update"),
				}),
			);

			// C sends to chat topic - only A should receive
			clientC.send(
				JSON.stringify({
					type: "chat",
					topic: topicChat,
					message: {
						id: "msg-chat",
						userId: "user-c",
						displayName: "User C",
						text: "Chat message",
						topic: topicChat,
						timestamp: new Date().toISOString(),
					},
				}),
			);

			const [receivedA, receivedB] = await Promise.all([messagesA, messagesB]);

			// Client A should only have chat messages
			const chatMsgsA = receivedA.filter((m) => m.type === "chat");
			const relayMsgsA = receivedA.filter((m) => m.type === "relay");
			expect(chatMsgsA.length).toBe(1);
			expect(relayMsgsA.length).toBe(0);

			// Client B should only have relay messages
			const chatMsgsB = receivedB.filter((m) => m.type === "chat");
			const relayMsgsB = receivedB.filter((m) => m.type === "relay");
			expect(chatMsgsB.length).toBe(0);
			expect(relayMsgsB.length).toBe(1);

			clientA.close();
			clientB.close();
			clientC.close();
		});
	});
});
