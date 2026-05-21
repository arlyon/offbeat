import { DurableObject } from "cloudflare:workers";
import { generateKeypair, sign, verify } from "./signing";

interface Session {
	topics: Set<string>;
}

/**
 * GossipWireMessage — the unified wire format used by both iroh-gossip
 * and the WS relay. All messages flowing through the DO use this shape.
 */
interface GossipWireMessage {
	kind: string;
	doc_id?: string;
	payload: string;
	group_key_id?: string;
}

interface CatchupEntry {
	seq: number;
	message: GossipWireMessage;
	timestamp: string;
}

export class FestivalDO extends DurableObject {
	#sessions = new Map<WebSocket, Session>();
	#publicKey: Uint8Array | null = null;
	#secretKey: Uint8Array | null = null;
	#opensAt: string | null = null; // ISO date string
	#closesAt: string | null = null; // ISO date string

	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);

		this.ctx.blockConcurrencyWhile(async () => {
			this.#initSchema();
			await this.#initKeypair();
			await this.#loadWindow();
		});
	}

	#initSchema() {
		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS gossip_log (
				seq INTEGER PRIMARY KEY AUTOINCREMENT,
				topic TEXT NOT NULL,
				message TEXT NOT NULL,
				timestamp TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE INDEX IF NOT EXISTS idx_gossip_topic_seq
				ON gossip_log(topic, seq);

			CREATE TABLE IF NOT EXISTS admins (
				public_key TEXT PRIMARY KEY
			);
		`);
	}

	async #initKeypair() {
		const stored = (await this.ctx.storage.get("ed25519_secret_key")) as Uint8Array | undefined;
		if (stored) {
			this.#secretKey = stored;
			const storedPub = (await this.ctx.storage.get("ed25519_public_key")) as Uint8Array;
			this.#publicKey = storedPub;
		} else {
			const { publicKey, secretKey } = generateKeypair();
			this.#publicKey = publicKey;
			this.#secretKey = secretKey;
			await this.ctx.storage.put("ed25519_secret_key", secretKey);
			await this.ctx.storage.put("ed25519_public_key", publicKey);
		}
	}

	async #loadWindow() {
		this.#opensAt = ((await this.ctx.storage.get("opens_at")) as string) ?? null;
		this.#closesAt = ((await this.ctx.storage.get("closes_at")) as string) ?? null;
	}

	/** Returns true if the current time is within the [opensAt, closesAt] window.
	 *  If no window is configured, always returns true (open by default). */
	#isWithinWindow(): boolean {
		if (!this.#opensAt || !this.#closesAt) return true;
		const now = new Date().toISOString();
		return now >= this.#opensAt && now <= this.#closesAt;
	}

	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);

		// Non-WS HTTP path: GET /public-key
		if (request.method === "GET" && url.pathname === "/public-key") {
			if (!this.#publicKey) {
				return new Response("Key not initialized", { status: 500 });
			}
			const hex = Array.from(this.#publicKey)
				.map((b) => b.toString(16).padStart(2, "0"))
				.join("");
			return new Response(hex, {
				headers: { "Content-Type": "text/plain" },
			});
		}

		// PUT /config — set the event window (opens_at, closes_at)
		if (request.method === "PUT" && url.pathname === "/config") {
			const body = (await request.json()) as {
				opensAt?: string;
				closesAt?: string;
			};
			if (body.opensAt) {
				this.#opensAt = body.opensAt;
				await this.ctx.storage.put("opens_at", body.opensAt);
			}
			if (body.closesAt) {
				this.#closesAt = body.closesAt;
				await this.ctx.storage.put("closes_at", body.closesAt);
			}
			return Response.json({
				opensAt: this.#opensAt,
				closesAt: this.#closesAt,
			});
		}

		// GET /config — read the current event window
		if (request.method === "GET" && url.pathname === "/config") {
			return Response.json({
				opensAt: this.#opensAt,
				closesAt: this.#closesAt,
			});
		}

		// PUT /admins — register an admin public key (hex-encoded Ed25519 verifying key).
		// First admin is auto-accepted (bootstrap). Subsequent admins require an
		// authenticated request from an existing admin.
		if (request.method === "PUT" && url.pathname === "/admins") {
			const body = (await request.json()) as {
				publicKey: string;
				signature?: string;
			};
			if (!body.publicKey || body.publicKey.length !== 64) {
				return new Response("publicKey must be 64 hex chars", { status: 400 });
			}

			const count = (
				this.sql.exec("SELECT COUNT(*) as cnt FROM admins").one() as {
					cnt: number;
				}
			).cnt;

			if (count > 0) {
				// Require proof from an existing admin
				if (!body.signature) {
					return new Response("Signature required from existing admin", {
						status: 401,
					});
				}
				// The existing admin signs the message "add-admin:{newPublicKey}"
				// and includes their own key in the Authorization header
				const authKey = request.headers.get("X-Admin-Key");
				if (!authKey) {
					return new Response("X-Admin-Key header required", { status: 401 });
				}
				const isAdmin =
					this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", authKey).toArray().length > 0;
				if (!isAdmin) {
					return new Response("Not an admin", { status: 403 });
				}
				const message = new TextEncoder().encode(`add-admin:${body.publicKey}`);
				const valid = await verify(hexToBytes(authKey), message, hexToBytes(body.signature));
				if (!valid) {
					return new Response("Invalid signature", { status: 401 });
				}
			}

			this.sql.exec("INSERT OR IGNORE INTO admins (public_key) VALUES (?)", body.publicKey);
			return Response.json({ ok: true });
		}

		// POST /signing-key — export the DO's Ed25519 signing (secret) key.
		// Caller must be a registered admin and prove it by signing the
		// message "export-signing-key" with their identity key.
		if (request.method === "POST" && url.pathname === "/signing-key") {
			const body = (await request.json()) as {
				publicKey: string;
				signature: string;
			};
			if (!body.publicKey || !body.signature) {
				return new Response("publicKey and signature required", {
					status: 400,
				});
			}

			// Check caller is admin
			const isAdmin =
				this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", body.publicKey).toArray()
					.length > 0;
			if (!isAdmin) {
				return new Response("Not an admin", { status: 403 });
			}

			// Verify signature over fixed challenge
			const message = new TextEncoder().encode("export-signing-key");
			const valid = await verify(hexToBytes(body.publicKey), message, hexToBytes(body.signature));
			if (!valid) {
				return new Response("Invalid signature", { status: 401 });
			}

			if (!this.#secretKey) {
				return new Response("Keypair not initialized", { status: 500 });
			}

			const hex = Array.from(this.#secretKey)
				.map((b) => b.toString(16).padStart(2, "0"))
				.join("");
			return new Response(hex, {
				headers: { "Content-Type": "text/plain" },
			});
		}

		// POST /sign-update — sign a Yrs update with the DO's key, broadcast it
		// to WS subscribers, and return the signed update. Requires admin auth.
		if (request.method === "POST" && url.pathname === "/sign-update") {
			const body = (await request.json()) as {
				publicKey: string;
				signature: string;
				docId: string;
				topic: string;
				update: string; // base64-encoded Yrs update bytes
			};
			if (!body.publicKey || !body.signature || !body.docId || !body.topic || !body.update) {
				return new Response("publicKey, signature, docId, topic, and update required", {
					status: 400,
				});
			}

			// Admin check
			const isAdmin =
				this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", body.publicKey).toArray()
					.length > 0;
			if (!isAdmin) {
				return new Response("Not an admin", { status: 403 });
			}

			// Verify caller signature over "sign-update:{docId}"
			const authMessage = new TextEncoder().encode(`sign-update:${body.docId}`);
			const authValid = await verify(
				hexToBytes(body.publicKey),
				authMessage,
				hexToBytes(body.signature),
			);
			if (!authValid) {
				return new Response("Invalid signature", { status: 401 });
			}

			if (!this.#secretKey || !this.#publicKey) {
				return new Response("Keypair not initialized", { status: 500 });
			}

			// Decode the update, sign it with the DO's key
			const updateBytes = base64ToBytes(body.update);
			const doSignature = await sign(this.#secretKey, updateBytes);

			const signedUpdate = {
				update: body.update,
				author: "festival-do",
				signature: bytesToBase64(doSignature),
			};

			// Build the gossip wire message
			const wireMessage: GossipWireMessage = {
				kind: "festival_update",
				doc_id: body.docId,
				payload: JSON.stringify(signedUpdate),
			};

			// Store in gossip log
			const msgData = JSON.stringify(wireMessage);
			const result = this.sql
				.exec(
					"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
					body.topic,
					msgData,
				)
				.one() as { seq: number };

			// Broadcast to subscribed WS clients
			const broadcast = JSON.stringify({
				type: "gossip",
				topic: body.topic,
				seq: result.seq,
				message: wireMessage,
			});
			for (const [ws, sess] of this.#sessions) {
				if (sess.topics.has(body.topic)) {
					ws.send(broadcast);
				}
			}

			return Response.json({
				seq: result.seq,
				signedUpdate,
				publicKey: Array.from(this.#publicKey)
					.map((b) => b.toString(16).padStart(2, "0"))
					.join(""),
			});
		}

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
			message?: GossipWireMessage;
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
			// Session not in memory — happens after hibernation, restore from attachment
			const raw = ws.deserializeAttachment() as string | null;
			const topics = new Set<string>(raw ? (JSON.parse(raw) as string[]) : []);
			sess = { topics };
			this.#sessions.set(ws, sess);
		}

		switch (parsed.type) {
			case "subscribe": {
				for (const topic of parsed.topics ?? []) {
					sess.topics.add(topic);
				}
				ws.serializeAttachment(JSON.stringify([...sess.topics]));
				ws.send(JSON.stringify({ type: "subscribed", topics: [...sess.topics] }));
				break;
			}

			case "unsubscribe": {
				for (const topic of parsed.topics ?? []) {
					sess.topics.delete(topic);
				}
				ws.serializeAttachment(JSON.stringify([...sess.topics]));
				ws.send(JSON.stringify({ type: "subscribed", topics: [...sess.topics] }));
				break;
			}

			case "gossip": {
				if (!parsed.topic || !parsed.message) break;

				if (!this.#isWithinWindow()) {
					ws.send(
						JSON.stringify({
							type: "error",
							error: "Event is not active — gossip rejected",
						}),
					);
					break;
				}

				const msgData = JSON.stringify(parsed.message);
				const result = this.sql
					.exec(
						"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
						parsed.topic,
						msgData,
					)
					.one() as { seq: number };

				const broadcast = JSON.stringify({
					type: "gossip",
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

			case "catchup": {
				if (!parsed.topic) break;
				const sinceSeq = parsed.sinceSeq ?? 0;

				const rows = this.sql
					.exec(
						"SELECT seq, message, timestamp FROM gossip_log WHERE topic = ? AND seq > ? ORDER BY seq",
						parsed.topic,
						sinceSeq,
					)
					.toArray() as { seq: number; message: string; timestamp: string }[];

				const messages: CatchupEntry[] = rows.map((r) => ({
					seq: r.seq,
					message: JSON.parse(r.message),
					timestamp: r.timestamp,
				}));

				ws.send(
					JSON.stringify({
						type: "catchup",
						topic: parsed.topic,
						messages,
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

	/**
	 * Sign arbitrary data with the DO's Ed25519 key.
	 * Used by admin endpoints to create trusted festival updates.
	 */
	async signUpdate(data: Uint8Array): Promise<{ signature: Uint8Array; publicKey: Uint8Array }> {
		if (!this.#secretKey || !this.#publicKey) {
			throw new Error("Keypair not initialized");
		}
		const signature = await sign(this.#secretKey, data);
		return { signature, publicKey: this.#publicKey };
	}

	/** Import admin keys from the central MainDO admin list. */
	async importAdmins(publicKeys: string[]) {
		for (const pk of publicKeys) {
			this.sql.exec("INSERT OR IGNORE INTO admins (public_key) VALUES (?)", pk);
		}
	}
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function hexToBytes(hex: string): Uint8Array {
	const bytes = new Uint8Array(hex.length / 2);
	for (let i = 0; i < hex.length; i += 2) {
		bytes[i / 2] = Number.parseInt(hex.substring(i, i + 2), 16);
	}
	return bytes;
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
