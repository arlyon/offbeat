import { DurableObject } from "cloudflare:workers";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
	ErrorCode,
	GossipEnvelopeSchema,
	type RelayClientMessage,
	RelayClientMessageSchema,
	RelayServerMessageSchema,
} from "@offbeat/protocol";
import * as Y from "yjs";
import { generateKeypair, sign, verify } from "./signing";

interface Session {
	topics: Set<string>;
	authenticated: boolean;
	publicKey: string | null;
}

export class FestivalDO extends DurableObject {
	#sessions = new Map<WebSocket, Session>();
	#publicKey: Uint8Array | null = null;
	#secretKey: Uint8Array | null = null;
	/** Hex-encoded 32-byte Ed25519 public key, used as the DO's deterministic endpoint_id. */
	#endpointId: string | null = null;
	#opensAt: string | null = null; // ISO date string
	#closesAt: string | null = null; // ISO date string
	#festivalId: string | null = null;
	#lat: number | null = null;
	#lon: number | null = null;

	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);

		this.ctx.blockConcurrencyWhile(async () => {
			this.#initSchema();
			await this.#initKeypair();
			await this.#loadConfig();
		});
	}

	#initSchema() {
		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS gossip_log (
				seq INTEGER PRIMARY KEY AUTOINCREMENT,
				topic TEXT NOT NULL,
				message BLOB NOT NULL,
				timestamp TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE INDEX IF NOT EXISTS idx_gossip_topic_seq
				ON gossip_log(topic, seq);

			CREATE TABLE IF NOT EXISTS admins (
				public_key TEXT PRIMARY KEY
			);

			CREATE TABLE IF NOT EXISTS yrs_docs (
				doc_id TEXT PRIMARY KEY,
				data BLOB NOT NULL,
				updated_at TEXT NOT NULL DEFAULT (datetime('now'))
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
		// Derive deterministic endpoint_id from public key
		this.#endpointId = bytesToHex(this.#publicKey);
	}

	async #loadConfig() {
		this.#opensAt = ((await this.ctx.storage.get("opens_at")) as string) ?? null;
		this.#closesAt = ((await this.ctx.storage.get("closes_at")) as string) ?? null;
		this.#festivalId = ((await this.ctx.storage.get("festival_id")) as string) ?? null;
		this.#lat = ((await this.ctx.storage.get("lat")) as number) ?? null;
		this.#lon = ((await this.ctx.storage.get("lon")) as number) ?? null;
	}

	/** Returns true if the current time is within the [opensAt, closesAt] window.
	 *  If no window is configured, always returns true (open by default). */
	#isWithinWindow(): boolean {
		if (!this.#opensAt || !this.#closesAt) return true;
		const now = new Date().toISOString();
		return now >= this.#opensAt && now <= this.#closesAt;
	}

	/** Send a binary RelayServerMessage to a WebSocket. */
	#sendServerMsg(
		ws: WebSocket,
		msg: Parameters<typeof create<typeof RelayServerMessageSchema>>[1],
	) {
		const serverMsg = create(RelayServerMessageSchema, msg);
		const bytes = toBinary(RelayServerMessageSchema, serverMsg);
		ws.send(bytes);
	}

	/** Send an error RelayServerMessage to a WebSocket. */
	#sendError(ws: WebSocket, error: string, code: ErrorCode = ErrorCode.UNSPECIFIED) {
		this.#sendServerMsg(ws, { msg: { case: "error", value: { error, code } } });
	}

	/**
	 * Verify attestation-based auth from request headers.
	 * Expects:
	 *   X-Attestation-Message: the attestation message string
	 *   X-Attestation-Signature: hex-encoded attestation signature
	 *   X-Attestation-Issuer: hex-encoded issuer public key
	 *   X-Session-PublicKey: hex-encoded Ed25519 public key
	 *   X-Session-Signature: hex-encoded session signature
	 *   X-Session-Timestamp: timestamp used in session signature
	 *
	 * Returns the user's public key hex on success, or an error Response.
	 */
	async #requireAuth(request: Request): Promise<{ publicKey: string } | Response> {
		const attMessage = request.headers.get("X-Attestation-Message");
		const attSignature = request.headers.get("X-Attestation-Signature");
		const attIssuer = request.headers.get("X-Attestation-Issuer");
		const sessionPubKey = request.headers.get("X-Session-PublicKey");
		const sessionSig = request.headers.get("X-Session-Signature");
		const sessionTimestamp = request.headers.get("X-Session-Timestamp");

		if (
			!attMessage ||
			!attSignature ||
			!attIssuer ||
			!sessionPubKey ||
			!sessionSig ||
			!sessionTimestamp
		) {
			return new Response("Auth headers required", { status: 401 });
		}

		// Verify attestation signature against issuer
		const attMsgBytes = new TextEncoder().encode(attMessage);
		const attValid = await verify(hexToBytes(attIssuer), attMsgBytes, hexToBytes(attSignature));
		if (!attValid) {
			return new Response("Invalid attestation signature", { status: 401 });
		}

		// Check attestation expiry (with 7-day grace period)
		const parts = attMessage.split(":");
		const expiresAt = Number.parseInt(parts[4], 10);
		const graceExpiry = expiresAt + 7 * 24 * 60 * 60;
		if (Date.now() / 1000 > graceExpiry) {
			return new Response("Attestation expired", { status: 401 });
		}

		// Verify the attestation binds to this public key
		const attPubKey = parts[2];
		if (attPubKey !== sessionPubKey) {
			return new Response("Attestation does not match session key", {
				status: 401,
			});
		}

		// Verify session signature (proves ownership of the Ed25519 key)
		const sessionMsg = new TextEncoder().encode(`session:${sessionTimestamp}`);
		const sessionValid = await verify(
			hexToBytes(sessionPubKey),
			sessionMsg,
			hexToBytes(sessionSig),
		);
		if (!sessionValid) {
			return new Response("Invalid session signature", { status: 401 });
		}

		return { publicKey: sessionPubKey };
	}

	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);

		// Non-WS HTTP path: GET /public-key
		if (request.method === "GET" && url.pathname === "/public-key") {
			if (!this.#publicKey) {
				return new Response("Key not initialized", { status: 500 });
			}
			return new Response(bytesToHex(this.#publicKey), {
				headers: { "Content-Type": "text/plain" },
			});
		}

		// PUT /config — set the event window and location
		if (request.method === "PUT" && url.pathname === "/config") {
			const body = (await request.json()) as {
				opensAt?: string;
				closesAt?: string;
				festivalId?: string;
				lat?: number;
				lon?: number;
			};
			if (body.opensAt) {
				this.#opensAt = body.opensAt;
				await this.ctx.storage.put("opens_at", body.opensAt);
			}
			if (body.closesAt) {
				this.#closesAt = body.closesAt;
				await this.ctx.storage.put("closes_at", body.closesAt);
			}
			if (body.festivalId) {
				this.#festivalId = body.festivalId;
				await this.ctx.storage.put("festival_id", body.festivalId);
			}
			if (body.lat !== undefined) {
				this.#lat = body.lat;
				await this.ctx.storage.put("lat", body.lat);
			}
			if (body.lon !== undefined) {
				this.#lon = body.lon;
				await this.ctx.storage.put("lon", body.lon);
			}
			return Response.json({
				opensAt: this.#opensAt,
				closesAt: this.#closesAt,
				festivalId: this.#festivalId,
				lat: this.#lat,
				lon: this.#lon,
			});
		}

		// GET /config — read the current config
		if (request.method === "GET" && url.pathname === "/config") {
			return Response.json({
				opensAt: this.#opensAt,
				closesAt: this.#closesAt,
				festivalId: this.#festivalId,
				lat: this.#lat,
				lon: this.#lon,
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

			return new Response(bytesToHex(this.#secretKey), {
				headers: { "Content-Type": "text/plain" },
			});
		}

		// DELETE /reset — wipe all storage and reinitialize
		if (request.method === "DELETE" && url.pathname === "/reset") {
			// Close all active WebSocket sessions
			for (const [ws] of this.#sessions) {
				ws.close(1000, "DO reset");
			}
			this.#sessions.clear();

			// Wipe all storage (SQL tables + KV)
			await this.ctx.storage.deleteAll();

			// Reinitialize
			this.#initSchema();
			await this.#initKeypair();

			return Response.json({ ok: true });
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

			// Build the GossipEnvelope as protobuf
			const envelope = create(GossipEnvelopeSchema, {
				payload: {
					case: "festivalUpdate",
					value: {
						docId: body.docId,
						signedUpdate: {
							update: updateBytes,
							author: "festival-do",
							signature: doSignature,
						},
					},
				},
			});

			// Store envelope as BLOB in gossip log
			const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
			const result = this.sql
				.exec(
					"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
					body.topic,
					envelopeBytes,
				)
				.one() as { seq: number };

			// Broadcast to subscribed WS clients
			const broadcastMsg = create(RelayServerMessageSchema, {
				msg: {
					case: "gossip",
					value: {
						topic: body.topic,
						seq: BigInt(result.seq),
						message: envelope,
					},
				},
			});
			const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);
			for (const [ws, sess] of this.#sessions) {
				if (sess.topics.has(body.topic)) {
					ws.send(broadcastBytes);
				}
			}

			return Response.json({
				seq: result.seq,
				signedUpdate: {
					update: body.update,
					author: "festival-do",
					signature: bytesToBase64(doSignature),
				},
				publicKey: bytesToHex(this.#publicKey),
			});
		}

		// POST /checkin — register a peer's endpoint ID in the CRDT
		if (request.method === "POST" && url.pathname === "/checkin") {
			const authResult = await this.#requireAuth(request);
			if (authResult instanceof Response) return authResult;
			const userId = authResult.publicKey;

			const body = (await request.json()) as {
				endpoint_id?: string;
				relay_url?: string | null;
			};

			if (!body.endpoint_id || !/^[0-9a-f]{64}$/.test(body.endpoint_id)) {
				return new Response("endpoint_id must be exactly 64 hex characters", { status: 400 });
			}

			if (!this.#festivalId) {
				return new Response("Festival not configured", { status: 500 });
			}

			const peerCount = await this.#writePeerCheckin(
				body.endpoint_id,
				body.relay_url ?? null,
				userId,
			);

			return Response.json({ ttl: 7200, peer_count: peerCount });
		}

		const upgradeHeader = request.headers.get("Upgrade");
		if (!upgradeHeader || upgradeHeader.toLowerCase() !== "websocket") {
			return new Response("Expected WebSocket upgrade", { status: 426 });
		}

		const { 0: client, 1: server } = new WebSocketPair();
		const sessionId = crypto.randomUUID();
		this.ctx.acceptWebSocket(server, [sessionId]);

		this.#sessions.set(server, { topics: new Set(), authenticated: false, publicKey: null });
		console.log(`[ws] new connection: ${sessionId}, total sessions: ${this.#sessions.size}`);

		// Send Hello message with the DO's deterministic endpoint_id
		if (this.#endpointId) {
			this.#sendServerMsg(server, {
				msg: { case: "hello", value: { endpointId: this.#endpointId } },
			});
		}

		// Store-and-forward: replay last N gossip messages per topic
		// so late joiners get caught up immediately on connect.
		this.#replayRecentMessages(server);

		return new Response(null, { status: 101, webSocket: client });
	}

	async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
		// Only accept binary frames
		if (typeof message === "string") {
			this.#sendError(ws, "Expected binary frame, not text", ErrorCode.MALFORMED);
			return;
		}

		const raw = new Uint8Array(message);
		console.log(`[ws] recv binary: ${raw.byteLength} bytes`);

		let parsed: RelayClientMessage;
		try {
			parsed = fromBinary(RelayClientMessageSchema, raw);
		} catch {
			this.#sendError(ws, "Invalid protobuf message", ErrorCode.MALFORMED);
			return;
		}

		let sess = this.#sessions.get(ws);
		if (!sess) {
			// Session not in memory — happens after hibernation, restore from attachment
			const rawAtt = ws.deserializeAttachment() as string | null;
			const attachment = rawAtt
				? (JSON.parse(rawAtt) as {
						topics?: string[];
						authenticated?: boolean;
						publicKey?: string | null;
					})
				: {};
			sess = {
				topics: new Set<string>(attachment.topics ?? []),
				authenticated: attachment.authenticated ?? false,
				publicKey: attachment.publicKey ?? null,
			};
			this.#sessions.set(ws, sess);
		}

		const { msg } = parsed;

		switch (msg.case) {
			case "auth": {
				const authData = msg.value;
				if (
					authData.publicKey.length === 0 ||
					!authData.attestation ||
					authData.signature.length === 0
				) {
					this.#sendError(ws, "Invalid auth message", ErrorCode.MALFORMED);
					break;
				}
				// Verify attestation signature against MainDO's public key (issuer)
				const attMsg = new TextEncoder().encode(authData.attestation.message);
				const attValid = await verify(
					authData.attestation.issuer,
					attMsg,
					authData.attestation.signature,
				);
				if (!attValid) {
					this.#sendError(ws, "Invalid attestation signature", ErrorCode.INVALID_SIGNATURE);
					break;
				}
				// Check attestation expiry (with 7-day grace period)
				const parts = authData.attestation.message.split(":");
				const expiresAt = Number.parseInt(parts[4], 10);
				const graceExpiry = expiresAt + 7 * 24 * 60 * 60;
				if (Date.now() / 1000 > graceExpiry) {
					this.#sendError(ws, "Attestation expired", ErrorCode.UNAUTHORIZED);
					break;
				}
				// Verify session signature (proves ownership of the Ed25519 key)
				const sessionMsg = new TextEncoder().encode(`session:${authData.timestamp}`);
				const sessionValid = await verify(authData.publicKey, sessionMsg, authData.signature);
				if (!sessionValid) {
					this.#sendError(ws, "Invalid session signature", ErrorCode.INVALID_SIGNATURE);
					break;
				}
				const publicKeyHex = bytesToHex(authData.publicKey);
				sess.authenticated = true;
				sess.publicKey = publicKeyHex;
				ws.serializeAttachment(
					JSON.stringify({
						topics: [...sess.topics],
						authenticated: true,
						publicKey: publicKeyHex,
					}),
				);
				const adminCount = (
					this.sql.exec("SELECT COUNT(*) as cnt FROM admins").one() as { cnt: number }
				).cnt;
				this.#sendServerMsg(ws, {
					msg: {
						case: "authOk",
						value: { authenticated: true, adminCount },
					},
				});
				break;
			}

			case "subscribe": {
				// Track which topics are genuinely new for this session
				const newTopics: string[] = [];
				for (const topic of msg.value.topics) {
					if (!sess.topics.has(topic)) {
						newTopics.push(topic);
					}
					sess.topics.add(topic);
				}
				ws.serializeAttachment(
					JSON.stringify({
						topics: [...sess.topics],
						authenticated: sess.authenticated,
						publicKey: sess.publicKey,
					}),
				);
				console.log(`[ws] subscribed to: ${[...sess.topics].join(", ")}`);
				this.#sendServerMsg(ws, {
					msg: { case: "subscribed", value: { topics: [...sess.topics] } },
				});
				// Replay recent messages for newly subscribed topics
				if (newTopics.length > 0) {
					this.#replayTopicMessages(ws, newTopics);
				}
				break;
			}

			case "unsubscribe": {
				for (const topic of msg.value.topics) {
					sess.topics.delete(topic);
				}
				ws.serializeAttachment(
					JSON.stringify({
						topics: [...sess.topics],
						authenticated: sess.authenticated,
						publicKey: sess.publicKey,
					}),
				);
				this.#sendServerMsg(ws, {
					msg: { case: "subscribed", value: { topics: [...sess.topics] } },
				});
				break;
			}

			case "gossip": {
				const { topic, message: envelope } = msg.value;
				if (!topic || !envelope) break;

				if (!sess.authenticated) {
					this.#sendError(ws, "Auth required for writes", ErrorCode.UNAUTHORIZED);
					break;
				}

				if (!this.#isWithinWindow()) {
					this.#sendError(ws, "Event is not active — gossip rejected", ErrorCode.UNAUTHORIZED);
					break;
				}

				// Store the GossipEnvelope as BLOB
				const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
				const result = this.sql
					.exec(
						"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
						topic,
						envelopeBytes,
					)
					.one() as { seq: number };

				// Broadcast to other subscribed WS clients
				const broadcastMsg = create(RelayServerMessageSchema, {
					msg: {
						case: "gossip",
						value: {
							topic,
							seq: BigInt(result.seq),
							message: envelope,
						},
					},
				});
				const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);

				for (const [other, otherSess] of this.#sessions) {
					if (other !== ws && otherSess.topics.has(topic)) {
						other.send(broadcastBytes);
					}
				}
				break;
			}

			case "catchup": {
				const { topic, sinceSeq } = msg.value;
				if (!topic) break;

				const rows = this.sql
					.exec(
						"SELECT seq, message, timestamp FROM gossip_log WHERE topic = ? AND seq > ? ORDER BY seq",
						topic,
						Number(sinceSeq),
					)
					.toArray() as { seq: number; message: ArrayBuffer; timestamp: string }[];

				const messages = rows.map((r) => ({
					seq: BigInt(r.seq),
					message: fromBinary(GossipEnvelopeSchema, new Uint8Array(r.message)),
					timestamp: r.timestamp,
				}));

				console.log(
					`[ws] catchup: topic=${topic} sinceSeq=${sinceSeq} sending ${messages.length} messages`,
				);
				this.#sendServerMsg(ws, {
					msg: { case: "catchup", value: { topic, messages } },
				});
				break;
			}

			case "svExchange": {
				const { docId, sv: clientSV } = msg.value;
				if (!docId || clientSV.length === 0) {
					this.#sendError(ws, "sv_exchange requires docId and sv", ErrorCode.MALFORMED);
					break;
				}

				// Determine topic from docId (e.g., "festival/fest-1/state")
				const topic = docId;

				// Load or create server doc
				const serverDoc = new Y.Doc();
				const stored = this.sql
					.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId)
					.toArray() as { data: ArrayBuffer }[];

				if (stored.length > 0) {
					Y.applyUpdate(serverDoc, new Uint8Array(stored[0].data));
				}

				// Apply any gossip_log entries that are festival_updates for this topic
				const logEntries = this.sql
					.exec("SELECT message FROM gossip_log WHERE topic = ? ORDER BY seq", topic)
					.toArray() as { message: ArrayBuffer }[];

				for (const entry of logEntries) {
					const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(entry.message));
					if (envelope.payload.case === "festivalUpdate" && envelope.payload.value.signedUpdate) {
						Y.applyUpdate(serverDoc, envelope.payload.value.signedUpdate.update);
					}
				}

				// Save the consolidated doc
				const fullState = Y.encodeStateAsUpdate(serverDoc);
				this.sql.exec(
					"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
					docId,
					fullState,
				);

				// Compute diff from client's state vector
				const diff = Y.encodeStateAsUpdate(serverDoc, clientSV);

				this.#sendServerMsg(ws, {
					msg: { case: "svDiff", value: { docId, diff } },
				});
				break;
			}

			case "chatCatchup": {
				const { topic: chatTopic, sv: chatSv, limit: chatLimit } = msg.value;
				if (!chatTopic) {
					this.#sendError(ws, "chat_catchup requires topic", ErrorCode.MALFORMED);
					break;
				}

				const maxLimit = chatLimit || 50;

				// Get chat messages from gossip_log for this topic
				const chatRows = this.sql
					.exec(
						"SELECT message FROM gossip_log WHERE topic = ? ORDER BY seq DESC LIMIT ?",
						chatTopic,
						maxLimit * 10,
					)
					.toArray() as { message: ArrayBuffer }[];

				// Filter: parse each envelope, extract userId and writerSeq (if chat)
				// Only include messages from writers not in sv, or with writerSeq > sv[writer]
				const chatMessages = [];
				for (const row of chatRows) {
					const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(row.message));
					if (envelope.payload.case === "chat") {
						const chatPayload = envelope.payload.value;
						const userId = chatPayload.userId;
						const writerSeq = chatPayload.writerSeq ?? 0n;
						if (userId && userId in chatSv) {
							if (writerSeq > chatSv[userId]) {
								chatMessages.push(envelope);
							}
						} else {
							chatMessages.push(envelope);
						}
					} else if (envelope.payload.case === "encryptedChat") {
						// Can't filter encrypted chat by writer — include all
						chatMessages.push(envelope);
					}
					if (chatMessages.length >= maxLimit) break;
				}

				this.#sendServerMsg(ws, {
					msg: {
						case: "chatDiff",
						value: { topic: chatTopic, messages: chatMessages },
					},
				});
				break;
			}

			default:
				this.#sendError(ws, `Unknown message case: ${msg.case}`, ErrorCode.MALFORMED);
		}
	}

	webSocketClose(ws: WebSocket): void {
		this.#sessions.delete(ws);
	}

	webSocketError(ws: WebSocket): void {
		this.#sessions.delete(ws);
	}

	// -----------------------------------------------------------------------
	// Store-and-forward: replay recent gossip on new connection / subscribe
	// -----------------------------------------------------------------------

	/**
	 * Replay the last 100 gossip messages (across all topics) to a newly
	 * connected WebSocket. This provides store-and-forward semantics so that
	 * clients joining mid-festival get caught up immediately without waiting
	 * for the next live broadcast.
	 *
	 * Called once on initial WS connection (before the client subscribes to
	 * specific topics). Since the client hasn't subscribed yet, messages are
	 * sent unfiltered; the client will ignore messages for topics it hasn't
	 * subscribed to internally.
	 */
	#replayRecentMessages(ws: WebSocket) {
		const rows = this.sql
			.exec("SELECT topic, seq, message FROM gossip_log ORDER BY seq DESC LIMIT 100")
			.toArray() as { topic: string; seq: number; message: ArrayBuffer }[];

		if (rows.length === 0) return;

		// Send oldest first (reverse the DESC order)
		for (let i = rows.length - 1; i >= 0; i--) {
			const row = rows[i];
			const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(row.message));
			this.#sendServerMsg(ws, {
				msg: {
					case: "gossip",
					value: {
						topic: row.topic,
						seq: BigInt(row.seq),
						message: envelope,
					},
				},
			});
		}

		console.log(`[ws] replayed ${rows.length} recent messages to new connection`);
	}

	/**
	 * Replay recent gossip messages for specific topics to a WebSocket.
	 * Called when a client subscribes to new topics, so they get caught up
	 * on messages they might have missed.
	 */
	#replayTopicMessages(ws: WebSocket, topics: string[]) {
		if (topics.length === 0) return;

		// Query last 100 messages per topic
		for (const topic of topics) {
			const rows = this.sql
				.exec(
					"SELECT seq, message FROM gossip_log WHERE topic = ? ORDER BY seq DESC LIMIT 100",
					topic,
				)
				.toArray() as { seq: number; message: ArrayBuffer }[];

			// Send oldest first
			for (let i = rows.length - 1; i >= 0; i--) {
				const row = rows[i];
				const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(row.message));
				this.#sendServerMsg(ws, {
					msg: {
						case: "gossip",
						value: {
							topic,
							seq: BigInt(row.seq),
							message: envelope,
						},
					},
				});
			}
		}
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

	/**
	 * Seed the Festival DO with lineup data as a signed Yrs CRDT document.
	 * Called by `ensureFestivalConfig` when the Festival DO is first initialised.
	 *
	 * Creates a Yrs doc with root-map keys "stages", "days", "sets" (JSON array
	 * strings), signs the update with the DO's Ed25519 key, and stores it in the
	 * gossip_log so that `sv_exchange` returns the lineup to connecting clients.
	 */
	async seedLineup(
		festivalId: string,
		lineup: {
			stages: { id: string; name: string; short: string; color: string; order: number }[];
			days: { id: string; label: string; num: number; month: string; year: number }[];
			sets: {
				id: string;
				day: string;
				stage: string;
				artist: string;
				startMin: number;
				durationMin: number;
				genre: string;
				cancelled: boolean;
			}[];
		},
	) {
		if (!this.#secretKey || !this.#publicKey) {
			throw new Error("Keypair not initialized");
		}

		const docId = `festival/${festivalId}/state`;
		const topic = docId;

		// Check if we already have a seeded doc — skip if so
		const existing = this.sql
			.exec("SELECT 1 FROM gossip_log WHERE topic = ? LIMIT 1", topic)
			.toArray();
		if (existing.length > 0) return;

		// Build a Yrs doc with the lineup data in the root map
		const doc = new Y.Doc();
		const root = doc.getMap("root");
		root.set("stages", JSON.stringify(lineup.stages));
		root.set("days", JSON.stringify(lineup.days));
		root.set("sets", JSON.stringify(lineup.sets));

		// Encode the full state as the update bytes
		const updateBytes = Y.encodeStateAsUpdate(doc);

		// Sign with the DO's Ed25519 key
		const signature = await sign(this.#secretKey, updateBytes);

		// Build the GossipEnvelope as protobuf
		const envelope = create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId,
					signedUpdate: {
						update: updateBytes,
						author: "festival-do",
						signature,
					},
				},
			},
		});

		// Store envelope as BLOB in gossip_log
		const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
		this.sql.exec("INSERT INTO gossip_log (topic, message) VALUES (?, ?)", topic, envelopeBytes);

		// Also persist the consolidated Yrs doc for faster sv_exchange
		const fullState = Y.encodeStateAsUpdate(doc);
		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);

		console.log(
			`[seedLineup] seeded ${docId}: ${lineup.stages.length} stages, ${lineup.days.length} days, ${lineup.sets.length} sets`,
		);
	}
	/**
	 * Update the Festival DO's Yrs CRDT document with a new lineup.
	 * Called when an admin refreshes the lineup via PUT /festivals/:id/lineup.
	 *
	 * Loads the existing Yrs doc, applies the new stages/days/sets as an
	 * incremental update, signs it, stores in gossip_log, persists to yrs_docs,
	 * and broadcasts to all subscribed WS clients.
	 *
	 * If no existing doc is found, falls through to seedLineup().
	 */
	async updateLineup(
		festivalId: string,
		lineup: {
			stages: { id: string; name: string; short: string; color: string; order: number }[];
			days: { id: string; label: string; num: number; month: string; year: number }[];
			sets: {
				id: string;
				day: string;
				stage: string;
				artist: string;
				startMin: number;
				durationMin: number;
				genre: string;
				cancelled: boolean;
			}[];
		},
	) {
		if (!this.#secretKey || !this.#publicKey) {
			throw new Error("Keypair not initialized");
		}

		const docId = `festival/${festivalId}/state`;
		const topic = docId;

		// If there's no existing doc, delegate to seedLineup for genesis
		const existing = this.sql
			.exec("SELECT 1 FROM gossip_log WHERE topic = ? LIMIT 1", topic)
			.toArray();
		if (existing.length === 0) {
			return this.seedLineup(festivalId, lineup);
		}

		// Load the existing Yrs doc
		const doc = new Y.Doc();
		const stored = this.sql.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId).toArray() as {
			data: ArrayBuffer;
		}[];

		if (stored.length > 0) {
			Y.applyUpdate(doc, new Uint8Array(stored[0].data));
		}

		// Replay gossip_log entries
		const logEntries = this.sql
			.exec("SELECT message FROM gossip_log WHERE topic = ? ORDER BY seq", topic)
			.toArray() as { message: ArrayBuffer }[];

		for (const entry of logEntries) {
			const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(entry.message));
			if (envelope.payload.case === "festivalUpdate" && envelope.payload.value.signedUpdate) {
				Y.applyUpdate(doc, envelope.payload.value.signedUpdate.update);
			}
		}

		// Capture state vector before mutation
		const prevSV = Y.encodeStateVector(doc);

		// Apply new lineup data
		const root = doc.getMap("root");
		root.set("stages", JSON.stringify(lineup.stages));
		root.set("days", JSON.stringify(lineup.days));
		root.set("sets", JSON.stringify(lineup.sets));

		// Encode incremental update
		const update = Y.encodeStateAsUpdate(doc, prevSV);

		// Sign with DO's Ed25519 key
		const signature = await sign(this.#secretKey, update);

		// Build the GossipEnvelope
		const envelope = create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId,
					signedUpdate: {
						update,
						author: "festival-do",
						signature,
					},
				},
			},
		});

		// Store in gossip_log
		const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
		const result = this.sql
			.exec(
				"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
				topic,
				envelopeBytes,
			)
			.one() as { seq: number };

		// Persist consolidated doc
		const fullState = Y.encodeStateAsUpdate(doc);
		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);

		// Broadcast to subscribed WS clients
		const broadcastMsg = create(RelayServerMessageSchema, {
			msg: {
				case: "gossip",
				value: {
					topic,
					seq: BigInt(result.seq),
					message: envelope,
				},
			},
		});
		const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);
		for (const [ws, sess] of this.#sessions) {
			if (sess.topics.has(topic)) {
				ws.send(broadcastBytes);
			}
		}

		console.log(
			`[updateLineup] updated ${docId}: ${lineup.stages.length} stages, ${lineup.days.length} days, ${lineup.sets.length} sets`,
		);
	}

	// -----------------------------------------------------------------------
	// Peer checkin helpers
	// -----------------------------------------------------------------------

	/**
	 * Write a peer checkin to the Yrs doc and broadcast the update.
	 * Also prunes stale entries (last_seen > 2 hours ago).
	 * Returns the count of active peers after the operation.
	 */
	async #writePeerCheckin(
		endpointId: string,
		relayUrl: string | null,
		userId: string,
	): Promise<number> {
		if (!this.#secretKey || !this.#publicKey || !this.#festivalId) {
			throw new Error("Not configured for peer checkin");
		}

		const docId = `festival/${this.#festivalId}/state`;
		const topic = docId;
		const nowSec = Math.floor(Date.now() / 1000);

		// Load existing Yrs doc
		const doc = new Y.Doc();
		const stored = this.sql.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId).toArray() as {
			data: ArrayBuffer;
		}[];

		if (stored.length > 0) {
			Y.applyUpdate(doc, new Uint8Array(stored[0].data));
		}

		// Replay gossip_log entries
		const logEntries = this.sql
			.exec("SELECT message FROM gossip_log WHERE topic = ? ORDER BY seq", topic)
			.toArray() as { message: ArrayBuffer }[];

		for (const entry of logEntries) {
			const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(entry.message));
			if (envelope.payload.case === "festivalUpdate" && envelope.payload.value.signedUpdate) {
				Y.applyUpdate(doc, envelope.payload.value.signedUpdate.update);
			}
		}

		// Get previous state vector for incremental diff
		const prevSV = Y.encodeStateVector(doc);

		// Get or create the "peers" YMap under root
		const root = doc.getMap("root");
		let peers = root.get("peers") as Y.Map<string> | undefined;
		if (!peers || !(peers instanceof Y.Map)) {
			peers = new Y.Map<string>();
			root.set("peers", peers);
		}

		// Set this peer's entry
		peers.set(
			endpointId,
			JSON.stringify({ relay_url: relayUrl, last_seen: nowSec, user_id: userId }),
		);

		// Prune stale entries (older than 2 hours)
		const cutoff = nowSec - 7200;
		const keysToDelete: string[] = [];
		for (const [key, value] of peers.entries()) {
			try {
				const entry = JSON.parse(value) as { last_seen: number };
				if (entry.last_seen < cutoff) {
					keysToDelete.push(key);
				}
			} catch {
				keysToDelete.push(key);
			}
		}
		for (const key of keysToDelete) {
			peers.delete(key);
		}

		const peerCount = peers.size;

		// Encode incremental update
		const update = Y.encodeStateAsUpdate(doc, prevSV);

		// Sign with DO's Ed25519 key
		const signature = await sign(this.#secretKey, update);

		// Build the GossipEnvelope as protobuf
		const envelope = create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId,
					signedUpdate: {
						update,
						author: "festival-do",
						signature,
					},
				},
			},
		});

		// Store envelope as BLOB in gossip_log
		const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
		const result = this.sql
			.exec(
				"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
				topic,
				envelopeBytes,
			)
			.one() as { seq: number };

		// Persist consolidated doc
		const fullState = Y.encodeStateAsUpdate(doc);
		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);

		// Broadcast to subscribed WS clients
		const broadcastMsg = create(RelayServerMessageSchema, {
			msg: {
				case: "gossip",
				value: {
					topic,
					seq: BigInt(result.seq),
					message: envelope,
				},
			},
		});
		const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);
		for (const [ws, sess] of this.#sessions) {
			if (sess.topics.has(topic)) {
				ws.send(broadcastBytes);
			}
		}

		console.log(
			`[writePeerCheckin] peer ${endpointId.slice(0, 8)}… checked in for ${docId}, ${peerCount} active peers`,
		);

		return peerCount;
	}

	/**
	 * Prune stale peer entries from the Yrs doc.
	 * Called by the alarm handler every 15 minutes.
	 */
	async #pruneStalePeers(): Promise<void> {
		if (!this.#secretKey || !this.#publicKey || !this.#festivalId) {
			return;
		}

		const docId = `festival/${this.#festivalId}/state`;
		const topic = docId;
		const nowSec = Math.floor(Date.now() / 1000);
		const cutoff = nowSec - 7200;

		// Load existing Yrs doc
		const doc = new Y.Doc();
		const stored = this.sql.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId).toArray() as {
			data: ArrayBuffer;
		}[];

		if (stored.length > 0) {
			Y.applyUpdate(doc, new Uint8Array(stored[0].data));
		}

		// Replay gossip_log entries
		const logEntries = this.sql
			.exec("SELECT message FROM gossip_log WHERE topic = ? ORDER BY seq", topic)
			.toArray() as { message: ArrayBuffer }[];

		for (const entry of logEntries) {
			const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(entry.message));
			if (envelope.payload.case === "festivalUpdate" && envelope.payload.value.signedUpdate) {
				Y.applyUpdate(doc, envelope.payload.value.signedUpdate.update);
			}
		}

		// Check if there's a peers map at all
		const root = doc.getMap("root");
		const peers = root.get("peers") as Y.Map<string> | undefined;
		if (!peers || !(peers instanceof Y.Map) || peers.size === 0) {
			return;
		}

		// Find stale entries
		const keysToDelete: string[] = [];
		for (const [key, value] of peers.entries()) {
			try {
				const entry = JSON.parse(value) as { last_seen: number };
				if (entry.last_seen < cutoff) {
					keysToDelete.push(key);
				}
			} catch {
				keysToDelete.push(key);
			}
		}

		if (keysToDelete.length === 0) {
			return;
		}

		// Get previous state vector for incremental diff
		const prevSV = Y.encodeStateVector(doc);

		// Delete stale entries
		for (const key of keysToDelete) {
			peers.delete(key);
		}

		// Encode incremental update
		const update = Y.encodeStateAsUpdate(doc, prevSV);

		// Sign with DO's Ed25519 key
		const signature = await sign(this.#secretKey, update);

		// Build the GossipEnvelope as protobuf
		const envelope = create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId,
					signedUpdate: {
						update,
						author: "festival-do",
						signature,
					},
				},
			},
		});

		// Store envelope as BLOB in gossip_log
		const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
		const result = this.sql
			.exec(
				"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
				topic,
				envelopeBytes,
			)
			.one() as { seq: number };

		// Persist consolidated doc
		const fullState = Y.encodeStateAsUpdate(doc);
		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);

		// Broadcast to subscribed WS clients
		const broadcastMsg = create(RelayServerMessageSchema, {
			msg: {
				case: "gossip",
				value: {
					topic,
					seq: BigInt(result.seq),
					message: envelope,
				},
			},
		});
		const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);
		for (const [ws, sess] of this.#sessions) {
			if (sess.topics.has(topic)) {
				ws.send(broadcastBytes);
			}
		}

		console.log(
			`[pruneStalePeers] pruned ${keysToDelete.length} stale peers for ${docId}, ${peers.size} remaining`,
		);
	}

	// -----------------------------------------------------------------------
	// Weather alarm + peer pruning
	// -----------------------------------------------------------------------

	/** DO alarm handler — fetches weather, prunes stale peers, and reschedules. */
	async alarm() {
		const now = new Date();

		// Guard: stop if past closesAt (festival over)
		if (this.#closesAt && now.toISOString() > this.#closesAt) {
			console.log("[alarm] festival closed, not rescheduling");
			return;
		}

		// Guard: if before opensAt - 24h, reschedule for that time
		if (this.#opensAt) {
			const earlyOpen = new Date(this.#opensAt);
			earlyOpen.setDate(earlyOpen.getDate() - 1);
			if (now < earlyOpen) {
				console.log(`[alarm] too early, rescheduling for ${earlyOpen.toISOString()}`);
				await this.ctx.storage.setAlarm(earlyOpen.getTime());
				return;
			}
		}

		// Always prune stale peers (every 15 min)
		try {
			await this.#pruneStalePeers();
		} catch (err) {
			console.error("[alarm] peer pruning failed:", err);
		}

		// Fetch weather every 6 hours (check last weather update time)
		if (this.#lat && this.#lon && this.#festivalId) {
			const lastWeather = (await this.ctx.storage.get("last_weather_alarm")) as number | undefined;
			const SIX_HOURS_MS = 6 * 60 * 60 * 1000;
			if (!lastWeather || now.getTime() - lastWeather >= SIX_HOURS_MS) {
				try {
					const weather = await this.#fetchWeather();
					await this.#writeWeatherToDoc(weather);
					await this.ctx.storage.put("last_weather_alarm", now.getTime());
					console.log(`[alarm] weather updated for ${this.#festivalId}`);
				} catch (err) {
					console.error("[alarm] weather fetch/write failed:", err);
				}
			}
		}

		// Reschedule in 15 minutes if still within window
		const FIFTEEN_MIN = 15 * 60 * 1000;
		const next = new Date(now.getTime() + FIFTEEN_MIN);
		if (!this.#closesAt || next.toISOString() <= this.#closesAt) {
			await this.ctx.storage.setAlarm(next.getTime());
			console.log(`[alarm] next alarm at ${next.toISOString()}`);
		}
	}

	/** Fetch hourly weather from Open-Meteo, capped to 1 day after festival closes. */
	async #fetchWeather(): Promise<WeatherData> {
		let forecastDays = 7;
		if (this.#closesAt) {
			const msLeft = new Date(this.#closesAt).getTime() - Date.now();
			forecastDays = Math.max(1, Math.min(7, Math.ceil(msLeft / 86_400_000)));
		}
		const url = `https://api.open-meteo.com/v1/forecast?latitude=${this.#lat}&longitude=${this.#lon}&hourly=temperature_2m,precipitation_probability,weather_code,wind_speed_10m&forecast_days=${forecastDays}&timezone=auto`;
		const resp = await fetch(url);
		if (!resp.ok) {
			throw new Error(`Open-Meteo error: ${resp.status} ${resp.statusText}`);
		}
		const data = (await resp.json()) as {
			timezone: string;
			hourly: {
				time: string[];
				temperature_2m: number[];
				precipitation_probability: number[];
				weather_code: number[];
				wind_speed_10m: number[];
			};
		};
		return {
			updatedAt: new Date().toISOString(),
			lat: this.#lat!,
			lon: this.#lon!,
			timezone: data.timezone,
			hourly: data.hourly,
		};
	}

	/** Merge weather into the Yrs doc and broadcast, following seedLineup pattern. */
	async #writeWeatherToDoc(weather: WeatherData) {
		if (!this.#secretKey || !this.#publicKey || !this.#festivalId) {
			throw new Error("Not configured for weather writes");
		}

		const docId = `festival/${this.#festivalId}/state`;
		const topic = docId;

		// Load existing Yrs doc
		const doc = new Y.Doc();
		const stored = this.sql.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId).toArray() as {
			data: ArrayBuffer;
		}[];

		if (stored.length > 0) {
			Y.applyUpdate(doc, new Uint8Array(stored[0].data));
		}

		// Replay gossip_log entries
		const logEntries = this.sql
			.exec("SELECT message FROM gossip_log WHERE topic = ? ORDER BY seq", topic)
			.toArray() as { message: ArrayBuffer }[];

		for (const entry of logEntries) {
			const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(entry.message));
			if (envelope.payload.case === "festivalUpdate" && envelope.payload.value.signedUpdate) {
				Y.applyUpdate(doc, envelope.payload.value.signedUpdate.update);
			}
		}

		// Merge weather: keep past entries, replace from "now" onwards
		const root = doc.getMap("root");
		const existingRaw = root.get("weather") as string | undefined;
		let merged = weather;

		if (existingRaw) {
			try {
				const existing = JSON.parse(existingRaw) as WeatherData;
				const nowIso = new Date().toISOString().slice(0, 16); // "YYYY-MM-DDTHH:MM"

				// Find cutoff: entries before "now" from existing, from "now" onwards from fresh
				const pastTimes: string[] = [];
				const pastTemp: number[] = [];
				const pastPrecip: number[] = [];
				const pastCode: number[] = [];
				const pastWind: number[] = [];

				for (let i = 0; i < existing.hourly.time.length; i++) {
					if (existing.hourly.time[i] < nowIso) {
						pastTimes.push(existing.hourly.time[i]);
						pastTemp.push(existing.hourly.temperature_2m[i]);
						pastPrecip.push(existing.hourly.precipitation_probability[i]);
						pastCode.push(existing.hourly.weather_code[i]);
						pastWind.push(existing.hourly.wind_speed_10m[i]);
					}
				}

				// Fresh entries from "now" onwards
				const freshTimes: string[] = [];
				const freshTemp: number[] = [];
				const freshPrecip: number[] = [];
				const freshCode: number[] = [];
				const freshWind: number[] = [];

				for (let i = 0; i < weather.hourly.time.length; i++) {
					if (weather.hourly.time[i] >= nowIso) {
						freshTimes.push(weather.hourly.time[i]);
						freshTemp.push(weather.hourly.temperature_2m[i]);
						freshPrecip.push(weather.hourly.precipitation_probability[i]);
						freshCode.push(weather.hourly.weather_code[i]);
						freshWind.push(weather.hourly.wind_speed_10m[i]);
					}
				}

				merged = {
					...weather,
					hourly: {
						time: [...pastTimes, ...freshTimes],
						temperature_2m: [...pastTemp, ...freshTemp],
						precipitation_probability: [...pastPrecip, ...freshPrecip],
						weather_code: [...pastCode, ...freshCode],
						wind_speed_10m: [...pastWind, ...freshWind],
					},
				};
			} catch {
				// If existing weather is corrupt, just use fresh data
			}
		}

		// Trim entries past closesAt
		if (this.#closesAt) {
			const cutoff = this.#closesAt.slice(0, 16); // "YYYY-MM-DDTHH:MM"
			const end = merged.hourly.time.findIndex((t) => t > cutoff);
			if (end !== -1) {
				merged.hourly.time = merged.hourly.time.slice(0, end);
				merged.hourly.temperature_2m = merged.hourly.temperature_2m.slice(0, end);
				merged.hourly.precipitation_probability = merged.hourly.precipitation_probability.slice(
					0,
					end,
				);
				merged.hourly.weather_code = merged.hourly.weather_code.slice(0, end);
				merged.hourly.wind_speed_10m = merged.hourly.wind_speed_10m.slice(0, end);
			}
		}

		// Get previous state vector for incremental diff
		const prevSV = Y.encodeStateVector(doc);

		// Write to Yrs doc
		root.set("weather", JSON.stringify(merged));

		// Encode incremental update
		const update = Y.encodeStateAsUpdate(doc, prevSV);

		// Sign with DO's Ed25519 key
		const signature = await sign(this.#secretKey, update);

		// Build the GossipEnvelope as protobuf
		const envelope = create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId,
					signedUpdate: {
						update,
						author: "festival-do",
						signature,
					},
				},
			},
		});

		// Store envelope as BLOB in gossip_log
		const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
		const result = this.sql
			.exec(
				"INSERT INTO gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
				topic,
				envelopeBytes,
			)
			.one() as { seq: number };

		// Persist consolidated doc
		const fullState = Y.encodeStateAsUpdate(doc);
		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);

		// Broadcast to subscribed WS clients
		const broadcastMsg = create(RelayServerMessageSchema, {
			msg: {
				case: "gossip",
				value: {
					topic,
					seq: BigInt(result.seq),
					message: envelope,
				},
			},
		});
		const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);
		for (const [ws, sess] of this.#sessions) {
			if (sess.topics.has(topic)) {
				ws.send(broadcastBytes);
			}
		}

		console.log(
			`[writeWeatherToDoc] wrote weather for ${docId}: ${merged.hourly.time.length} hourly entries`,
		);
	}

	/** Arm the weather alarm. If no alarm is set, triggers immediately. */
	async armWeatherAlarm() {
		const existing = await this.ctx.storage.getAlarm();
		if (existing) {
			console.log(`[armWeatherAlarm] alarm already set for ${new Date(existing).toISOString()}`);
			return;
		}
		await this.ctx.storage.setAlarm(Date.now());
		console.log("[armWeatherAlarm] armed — triggering immediately");
	}
}

/** Shape of weather data stored in the Yrs doc. */
interface WeatherData {
	updatedAt: string;
	lat: number;
	lon: number;
	timezone: string;
	hourly: {
		time: string[];
		temperature_2m: number[];
		precipitation_probability: number[];
		weather_code: number[];
		wind_speed_10m: number[];
	};
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

function bytesToHex(bytes: Uint8Array): string {
	return Array.from(bytes)
		.map((b) => b.toString(16).padStart(2, "0"))
		.join("");
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
