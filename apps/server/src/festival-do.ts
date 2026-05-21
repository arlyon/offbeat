import { DurableObject } from "cloudflare:workers";
import type { ChatMessage } from "@offbeat/protocol";

interface Session {
	topics: Set<string>;
}

export class FestivalDO extends DurableObject {
	#sessions = new Map<WebSocket, Session>();

	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);

		this.ctx.blockConcurrencyWhile(async () => {
			this.#initSchema();
		});
	}

	#initSchema() {
		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS chat_log (
				seq INTEGER PRIMARY KEY AUTOINCREMENT,
				topic TEXT NOT NULL,
				data TEXT NOT NULL,
				timestamp TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE TABLE IF NOT EXISTS relay_log (
				seq INTEGER PRIMARY KEY AUTOINCREMENT,
				topic TEXT NOT NULL,
				data TEXT NOT NULL,
				timestamp TEXT NOT NULL DEFAULT (datetime('now'))
			);
		`);
	}

	async fetch(request: Request): Promise<Response> {
		const upgradeHeader = request.headers.get("Upgrade");
		if (!upgradeHeader || upgradeHeader.toLowerCase() !== "websocket") {
			return new Response("Expected WebSocket upgrade", { status: 426 });
		}

		const { 0: client, 1: server } = new WebSocketPair();
		const sessionId = crypto.randomUUID();
		this.ctx.acceptWebSocket(server, [sessionId]);

		this.#sessions.set(server, { topics: new Set() });

		return new Response(null, { status: 101, webSocket: client });
	}

	webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): void {
		let parsed: {
			type: string;
			topics?: string[];
			topic?: string;
			message?: ChatMessage;
			data?: string;
			sinceSeq?: number;
		};

		try {
			parsed = JSON.parse(
				typeof message === "string" ? message : new TextDecoder().decode(message),
			);
		} catch {
			ws.send(JSON.stringify({ type: "error", error: "Invalid JSON" }));
			return;
		}

		let sess = this.#sessions.get(ws);
		if (!sess) {
			// Session not in memory — happens after hibernation, reconstruct it
			sess = { topics: new Set() };
			this.#sessions.set(ws, sess);
		}

		switch (parsed.type) {
			case "subscribe": {
				for (const topic of parsed.topics ?? []) {
					sess.topics.add(topic);
				}
				ws.send(JSON.stringify({ type: "subscribed", topics: [...sess.topics] }));
				break;
			}

			case "unsubscribe": {
				for (const topic of parsed.topics ?? []) {
					sess.topics.delete(topic);
				}
				ws.send(JSON.stringify({ type: "subscribed", topics: [...sess.topics] }));
				break;
			}

			case "chat": {
				if (!parsed.topic || !parsed.message) break;
				const chatData = JSON.stringify(parsed.message);
				const result = this.sql
					.exec(
						"INSERT INTO chat_log (topic, data) VALUES (?, ?) RETURNING seq",
						parsed.topic,
						chatData,
					)
					.one() as { seq: number };

				const broadcast = JSON.stringify({
					type: "chat",
					topic: parsed.topic,
					seq: result.seq,
					message: parsed.message,
				});

				for (const [other, otherSess] of this.#sessions) {
					if (other !== ws && otherSess.topics.has(parsed.topic)) {
						other.send(broadcast);
					}
				}
				break;
			}

			case "relay": {
				if (!parsed.topic || !parsed.data) break;
				const result = this.sql
					.exec(
						"INSERT INTO relay_log (topic, data) VALUES (?, ?) RETURNING seq",
						parsed.topic,
						parsed.data,
					)
					.one() as { seq: number };

				const broadcast = JSON.stringify({
					type: "relay",
					topic: parsed.topic,
					seq: result.seq,
					data: parsed.data,
				});

				for (const [other, otherSess] of this.#sessions) {
					if (other !== ws && otherSess.topics.has(parsed.topic)) {
						other.send(broadcast);
					}
				}
				break;
			}

			case "catchup": {
				if (!parsed.topic) break;
				const sinceSeq = parsed.sinceSeq ?? 0;

				const chatRows = this.sql
					.exec(
						"SELECT seq, data, timestamp FROM chat_log WHERE topic = ? AND seq > ? ORDER BY seq",
						parsed.topic,
						sinceSeq,
					)
					.toArray() as { seq: number; data: string; timestamp: string }[];

				const relayRows = this.sql
					.exec(
						"SELECT seq, data, timestamp FROM relay_log WHERE topic = ? AND seq > ? ORDER BY seq",
						parsed.topic,
						sinceSeq,
					)
					.toArray() as { seq: number; data: string; timestamp: string }[];

				ws.send(
					JSON.stringify({
						type: "catchup",
						topic: parsed.topic,
						chat: chatRows.map((r) => ({
							seq: r.seq,
							message: JSON.parse(r.data),
							timestamp: r.timestamp,
						})),
						relay: relayRows.map((r) => ({
							seq: r.seq,
							data: r.data,
							timestamp: r.timestamp,
						})),
					}),
				);
				break;
			}

			default:
				ws.send(JSON.stringify({ type: "error", error: `Unknown type: ${parsed.type}` }));
		}
	}

	webSocketClose(ws: WebSocket): void {
		this.#sessions.delete(ws);
	}

	webSocketError(ws: WebSocket): void {
		this.#sessions.delete(ws);
	}
}
