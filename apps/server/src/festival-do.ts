import { DurableObject } from "cloudflare:workers";
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
	ErrorCode,
	FestivalUpdateKind,
	GossipEnvelopeSchema,
	type RelayClientMessage,
	RelayClientMessageSchema,
	RelayServerMessageSchema,
	type ChatMessage as WireChatMessage,
} from "@offbeat/protocol";
import * as Y from "yjs";
import { generateKeypair, sign, signFestivalUpdate, verify } from "./signing";

const RELAY_ACK_CAPABILITY_TOPIC = "__offbeat/relay-ack/v1";
const MAX_CHAT_CATCHUP = 100;
const MAX_CHAT_STATE_WRITERS = 4096;
const MAX_CHAT_MESSAGE_BYTES = 64 * 1024;
const MAX_CHAT_CATCHUP_BYTES = 512 * 1024;
const MAX_SEQUENCE_CATCHUP = 100;
const MAX_SEQUENCE_CATCHUP_BYTES = 1024 * 1024;
const MAX_CLIENT_FRAME_BYTES = 512 * 1024;
const MAX_REMOTE_LAMPORT_ADVANCE = 1_000_000n;
const EQUIVOCATED_HEAD_ID = "__offbeat/equivocated__";

interface Session {
	topics: Set<string>;
	authenticated: boolean;
	publicKey: string | null;
}

interface ChatMetadata {
	writerId: string;
	writerSeq: number;
	messageId: string;
	logicalTime: number;
}

interface LegacyPublicRow {
	seq: number;
	topic: string;
	message: ArrayBuffer;
}

type ChatValidation = { metadata: ChatMetadata } | { error: string; code: ErrorCode };

function checkStableChatRepair(stored: ArrayBuffer, incoming: WireChatMessage) {
	const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(stored));
	const current = envelope.payload.case === "chat" ? envelope.payload.value : undefined;
	const immutableFieldsMatch =
		current !== undefined &&
		current.id === incoming.id &&
		current.userId === incoming.userId &&
		current.writerSeq === incoming.writerSeq &&
		current.displayName === incoming.displayName &&
		current.text === incoming.text &&
		current.topic === incoming.topic &&
		current.stageId === incoming.stageId &&
		current.timestamp === incoming.timestamp;
	const repairsFallback = current?.logicalTime === 0n && incoming.logicalTime > 0n;
	const retriesFallback =
		current !== undefined && current.logicalTime > 0n && incoming.logicalTime === 0n;
	return {
		envelope,
		repairsFallback,
		valid:
			immutableFieldsMatch &&
			(current.logicalTime === incoming.logicalTime || repairsFallback || retriesFallback),
	};
}

function isPublicChatTopic(topic: string, festivalId: string | null) {
	return festivalId !== null && topic.startsWith(`festival/${festivalId}/chat/`);
}

function committedLogicalTime(commitment: string) {
	const separator = commitment.lastIndexOf("@");
	if (separator < 0) return 0n;
	try {
		return BigInt(commitment.slice(separator + 1));
	} catch {
		return 0n;
	}
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
	#publicStateDoc: Y.Doc | null = null;

	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);

		this.ctx.blockConcurrencyWhile(async () => {
			this.#initSchema();
			await this.#initKeypair();
			await this.#loadConfig();
			this.#loadPublicStateDoc();
		});
	}

	#initSchema() {
		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS public_gossip_log (
				seq INTEGER PRIMARY KEY AUTOINCREMENT,
				topic TEXT NOT NULL,
				message BLOB NOT NULL,
				timestamp TEXT NOT NULL DEFAULT (datetime('now')),
				writer_id TEXT,
				writer_seq INTEGER,
				message_id TEXT,
				logical_time INTEGER
			);

			CREATE INDEX IF NOT EXISTS idx_public_gossip_topic_seq
				ON public_gossip_log(topic, seq);

			CREATE TABLE IF NOT EXISTS chat_catchup_heads (
				request_id TEXT NOT NULL,
				writer_id TEXT NOT NULL,
				writer_seq INTEGER NOT NULL,
				head_id TEXT NOT NULL,
				PRIMARY KEY(request_id, writer_id)
			);

			CREATE TABLE IF NOT EXISTS group_gossip_log (
				seq INTEGER PRIMARY KEY AUTOINCREMENT,
				group_id TEXT NOT NULL,
				message BLOB NOT NULL,
				timestamp TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE INDEX IF NOT EXISTS idx_group_gossip_group_seq
				ON group_gossip_log(group_id, seq);

			CREATE TABLE IF NOT EXISTS relay_receipts (
				topic TEXT NOT NULL,
				message BLOB NOT NULL,
				seq INTEGER NOT NULL,
				PRIMARY KEY (topic, message)
			);

			CREATE TABLE IF NOT EXISTS group_yrs_updates (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				group_id TEXT NOT NULL,
				client_id TEXT,
				sequence_number INTEGER,
				update_data BLOB NOT NULL
			);

			CREATE INDEX IF NOT EXISTS idx_group_yrs_group
				ON group_yrs_updates(group_id);

			CREATE TABLE IF NOT EXISTS admins (
				public_key TEXT PRIMARY KEY
			);

			CREATE TABLE IF NOT EXISTS yrs_docs (
				doc_id TEXT PRIMARY KEY,
				data BLOB NOT NULL,
				updated_at TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE TABLE IF NOT EXISTS festival_signed_updates (
				doc_id TEXT NOT NULL,
				authority_seq INTEGER NOT NULL,
				kind INTEGER NOT NULL,
				update_data BLOB NOT NULL,
				signature BLOB NOT NULL,
				created_at TEXT NOT NULL DEFAULT (datetime('now')),
				PRIMARY KEY (doc_id, authority_seq, kind)
			);

			CREATE INDEX IF NOT EXISTS idx_festival_signed_checkpoint
				ON festival_signed_updates(doc_id, kind, authority_seq DESC);

			DROP TABLE IF EXISTS gossip_log;
		`);
		this.#migratePublicChatMetadata();
	}

	#migratePublicChatMetadata() {
		const columns = new Set(
			(this.sql.exec("PRAGMA table_info(public_gossip_log)").toArray() as { name: string }[]).map(
				(column) => column.name,
			),
		);
		if (!columns.has("writer_id")) {
			this.sql.exec("ALTER TABLE public_gossip_log ADD COLUMN writer_id TEXT");
		}
		if (!columns.has("writer_seq")) {
			this.sql.exec("ALTER TABLE public_gossip_log ADD COLUMN writer_seq INTEGER");
		}
		if (!columns.has("message_id")) {
			this.sql.exec("ALTER TABLE public_gossip_log ADD COLUMN message_id TEXT");
		}
		if (!columns.has("logical_time")) {
			this.sql.exec("ALTER TABLE public_gossip_log ADD COLUMN logical_time INTEGER");
		}
		this.sql.exec(
			"CREATE INDEX IF NOT EXISTS idx_public_chat_order ON public_gossip_log(topic, logical_time, writer_id, writer_seq, message_id)",
		);

		const legacy = this.sql
			.exec(
				"SELECT seq, topic, message FROM public_gossip_log WHERE writer_id IS NULL ORDER BY seq",
			)
			.toArray() as unknown as LegacyPublicRow[];
		const writerMax = this.#collectLegacyWriterMax(legacy);
		for (const row of legacy) this.#migrateLegacyPublicChat(row, writerMax);
		this.sql.exec(`
			DELETE FROM public_gossip_log AS stale
			WHERE stale.message_id IS NOT NULL AND EXISTS (
				SELECT 1 FROM public_gossip_log AS preferred
				WHERE preferred.topic = stale.topic
				  AND preferred.message_id = stale.message_id
				  AND (
					preferred.logical_time > stale.logical_time OR
					(preferred.logical_time = stale.logical_time AND preferred.seq > stale.seq)
				  )
			);
			CREATE UNIQUE INDEX IF NOT EXISTS idx_public_chat_message_id
				ON public_gossip_log(topic, message_id) WHERE message_id IS NOT NULL;
		`);
	}

	#decodeLegacyPublicChat(row: LegacyPublicRow) {
		try {
			const envelope = fromBinary(GossipEnvelopeSchema, new Uint8Array(row.message));
			if (envelope.payload.case !== "chat" || envelope.payload.value.topic !== row.topic) {
				return undefined;
			}
			return { envelope, chat: envelope.payload.value };
		} catch {
			return undefined;
		}
	}

	#collectLegacyWriterMax(rows: LegacyPublicRow[]) {
		const writerMax = new Map<string, bigint>();
		for (const row of rows) {
			const decoded = this.#decodeLegacyPublicChat(row);
			if (!decoded) continue;
			const { chat } = decoded;
			if (chat.writerSeq === 0n || chat.writerSeq > MAX_REMOTE_LAMPORT_ADVANCE) continue;
			const key = `${row.topic}\u0000${chat.userId}`;
			const current = writerMax.get(key) ?? 0n;
			if (chat.writerSeq > current) writerMax.set(key, chat.writerSeq);
		}
		return writerMax;
	}

	#migrateLegacyPublicChat(row: LegacyPublicRow, writerMax: Map<string, bigint>) {
		const decoded = this.#decodeLegacyPublicChat(row);
		if (!decoded) return;
		const { envelope, chat } = decoded;
		if (chat.writerSeq === 0n) {
			const key = `${row.topic}\u0000${chat.userId}`;
			chat.writerSeq = (writerMax.get(key) ?? 0n) + 1n;
			writerMax.set(key, chat.writerSeq);
			if (chat.logicalTime === 0n) chat.logicalTime = chat.writerSeq;
		}
		const logicalTime = chat.logicalTime === 0n ? chat.writerSeq : chat.logicalTime;
		if (
			chat.writerSeq > BigInt(Number.MAX_SAFE_INTEGER) ||
			logicalTime > BigInt(Number.MAX_SAFE_INTEGER)
		) {
			return;
		}
		const currentLogicalTime = BigInt(
			(
				this.sql
					.exec(
						"SELECT COALESCE(MAX(logical_time), 0) AS value FROM public_gossip_log WHERE topic = ?",
						row.topic,
					)
					.one() as { value: number }
			).value,
		);
		if (logicalTime > currentLogicalTime + MAX_REMOTE_LAMPORT_ADVANCE) return;
		this.sql.exec(
			"UPDATE public_gossip_log SET message = ?, writer_id = ?, writer_seq = ?, message_id = ?, logical_time = ? WHERE seq = ?",
			toBinary(GossipEnvelopeSchema, envelope),
			chat.userId,
			Number(chat.writerSeq),
			chat.id,
			Number(logicalTime),
			row.seq,
		);
	}

	/** Load the public state doc from yrs_docs into memory at boot. */
	#loadPublicStateDoc() {
		if (!this.#festivalId) return;
		const docId = `festival/${this.#festivalId}/state`;
		const doc = new Y.Doc();
		const stored = this.sql.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId).toArray() as {
			data: ArrayBuffer;
		}[];
		if (stored.length > 0) {
			Y.applyUpdate(doc, new Uint8Array(stored[0].data));
		}
		this.#publicStateDoc = doc;
	}

	/**
	 * Mutate the in-memory public state doc, sign the delta, persist, and broadcast.
	 * Shared helper for all festival state mutations (lineup, checkin, weather, prune).
	 */
	async #mutatePublicDoc(mutate: (doc: Y.Doc) => void) {
		if (!this.#secretKey || !this.#publicKey || !this.#festivalId) {
			throw new Error("Not configured for public doc mutation");
		}

		const docId = `festival/${this.#festivalId}/state`;
		const topic = docId;

		if (!this.#publicStateDoc) {
			// Load from disk if available (e.g. after hibernation or late config)
			const doc = new Y.Doc();
			const stored = this.sql
				.exec("SELECT data FROM yrs_docs WHERE doc_id = ?", docId)
				.toArray() as { data: ArrayBuffer }[];
			if (stored.length > 0) {
				Y.applyUpdate(doc, new Uint8Array(stored[0].data));
			}
			this.#publicStateDoc = doc;
		}

		const doc = this.#publicStateDoc;
		const prevSV = Y.encodeStateVector(doc);

		mutate(doc);

		const update = Y.encodeStateAsUpdate(doc, prevSV);
		const fullState = Y.encodeStateAsUpdate(doc);
		const authoritySeq = this.#nextAuthoritySeq(docId);
		const deltaEnvelope = await this.#persistSignedFestivalUpdate(
			docId,
			FestivalUpdateKind.DELTA,
			authoritySeq,
			update,
		);
		await this.#persistSignedFestivalUpdate(
			docId,
			FestivalUpdateKind.CHECKPOINT,
			authoritySeq,
			fullState,
		);

		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);

		// Live subscribers receive the small signed delta. Late joiners request
		// the signed checkpoint through svExchange.
		const broadcastMsg = create(RelayServerMessageSchema, {
			msg: {
				case: "gossip",
				value: {
					topic,
					seq: 0n,
					message: deltaEnvelope,
				},
			},
		});
		const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);
		for (const [ws, sess] of this.#sessions) {
			if (sess.topics.has(topic)) {
				ws.send(broadcastBytes);
			}
		}
		return deltaEnvelope;
	}

	#nextAuthoritySeq(docId: string): bigint {
		const row = this.sql
			.exec(
				"SELECT COALESCE(MAX(authority_seq), 0) AS seq FROM festival_signed_updates WHERE doc_id = ?",
				docId,
			)
			.one() as { seq: number };
		return BigInt(row.seq) + 1n;
	}

	async #persistSignedFestivalUpdate(
		docId: string,
		kind: FestivalUpdateKind,
		authoritySeq: bigint,
		update: Uint8Array,
	) {
		if (!this.#secretKey) throw new Error("Festival signing key is unavailable");
		if (authoritySeq > BigInt(Number.MAX_SAFE_INTEGER)) {
			throw new Error("Festival authority sequence exceeds SQLite integer precision");
		}
		const signature = await signFestivalUpdate(this.#secretKey, docId, kind, authoritySeq, update);
		this.sql.exec(
			`INSERT OR REPLACE INTO festival_signed_updates
				(doc_id, authority_seq, kind, update_data, signature)
			 VALUES (?, ?, ?, ?, ?)`,
			docId,
			Number(authoritySeq),
			kind,
			update,
			signature,
		);

		if (kind === FestivalUpdateKind.CHECKPOINT) {
			this.sql.exec(
				"DELETE FROM festival_signed_updates WHERE doc_id = ? AND kind = ? AND authority_seq < ?",
				docId,
				FestivalUpdateKind.CHECKPOINT,
				Number(authoritySeq),
			);
		} else {
			const retainAfter = Math.max(0, Number(authoritySeq) - 256);
			this.sql.exec(
				"DELETE FROM festival_signed_updates WHERE doc_id = ? AND kind = ? AND authority_seq < ?",
				docId,
				FestivalUpdateKind.DELTA,
				retainAfter,
			);
		}

		return create(GossipEnvelopeSchema, {
			payload: {
				case: "festivalUpdate",
				value: {
					docId,
					kind,
					authoritySeq,
					signedUpdate: {
						update,
						author: "festival-do",
						signature,
					},
				},
			},
		});
	}

	async #ensureSignedCheckpoint(docId: string) {
		const rows = this.sql
			.exec(
				`SELECT authority_seq, update_data, signature
				 FROM festival_signed_updates
				 WHERE doc_id = ? AND kind = ?
				 ORDER BY authority_seq DESC LIMIT 1`,
				docId,
				FestivalUpdateKind.CHECKPOINT,
			)
			.toArray() as { authority_seq: number; update_data: ArrayBuffer; signature: ArrayBuffer }[];
		const latest = rows[0];
		if (latest) {
			return create(GossipEnvelopeSchema, {
				payload: {
					case: "festivalUpdate",
					value: {
						docId,
						kind: FestivalUpdateKind.CHECKPOINT,
						authoritySeq: BigInt(latest.authority_seq),
						signedUpdate: {
							update: new Uint8Array(latest.update_data),
							author: "festival-do",
							signature: new Uint8Array(latest.signature),
						},
					},
				},
			});
		}

		if (!this.#publicStateDoc) this.#loadPublicStateDoc();
		const doc = this.#publicStateDoc ?? new Y.Doc();
		const fullState = Y.encodeStateAsUpdate(doc);
		return this.#persistSignedFestivalUpdate(
			docId,
			FestivalUpdateKind.CHECKPOINT,
			this.#nextAuthoritySeq(docId),
			fullState,
		);
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

	#validatePublicChat(topic: string, chat: WireChatMessage, session: Session): ChatValidation {
		const expectedPrefix = session.publicKey?.slice(0, 16);
		if (chat.topic !== topic || !topic.startsWith(`festival/${this.#festivalId}/chat/`)) {
			return { error: "Chat topic mismatch", code: ErrorCode.MALFORMED };
		}
		if (chat.userId !== session.publicKey && chat.userId !== expectedPrefix) {
			return { error: "Chat writer does not match session", code: ErrorCode.UNAUTHORIZED };
		}
		const logicalTime = chat.logicalTime === 0n ? chat.writerSeq : chat.logicalTime;
		if (
			chat.writerSeq === 0n ||
			chat.writerSeq > BigInt(Number.MAX_SAFE_INTEGER) ||
			logicalTime > BigInt(Number.MAX_SAFE_INTEGER)
		) {
			return { error: "Chat order exceeds relay range", code: ErrorCode.MALFORMED };
		}
		const currentWriterSequence = BigInt(
			(
				this.sql
					.exec(
						"SELECT COALESCE(MAX(writer_seq), 0) AS value FROM public_gossip_log WHERE topic = ? AND writer_id = ?",
						topic,
						chat.userId,
					)
					.one() as { value: number }
			).value,
		);
		if (chat.writerSeq > currentWriterSequence + MAX_REMOTE_LAMPORT_ADVANCE) {
			return { error: "Chat writer sequence advance is too large", code: ErrorCode.MALFORMED };
		}
		const currentLogicalTime = BigInt(
			(
				this.sql
					.exec(
						"SELECT COALESCE(MAX(logical_time), 0) AS value FROM public_gossip_log WHERE topic = ?",
						topic,
					)
					.one() as { value: number }
			).value,
		);
		if (logicalTime > currentLogicalTime + MAX_REMOTE_LAMPORT_ADVANCE) {
			return { error: "Chat Lamport advance is too large", code: ErrorCode.MALFORMED };
		}
		return {
			metadata: {
				writerId: chat.userId,
				writerSeq: Number(chat.writerSeq),
				messageId: chat.id,
				logicalTime: Number(logicalTime),
			},
		};
	}

	#sendCatchup(ws: WebSocket, topic: string, sinceSeq: bigint) {
		if (sinceSeq > BigInt(Number.MAX_SAFE_INTEGER)) {
			this.#sendError(ws, "Catch-up sequence exceeds relay range", ErrorCode.MALFORMED);
			return;
		}
		if (isPublicChatTopic(topic, this.#festivalId)) {
			this.#sendError(ws, "Public chat requires committed-head catch-up", ErrorCode.MALFORMED);
			return;
		}
		const rows = (
			topic.startsWith("festival/")
				? this.sql.exec(
						"SELECT seq, message, timestamp FROM public_gossip_log WHERE topic = ? AND seq > ? ORDER BY seq LIMIT ?",
						topic,
						Number(sinceSeq),
						MAX_SEQUENCE_CATCHUP,
					)
				: this.sql.exec(
						"SELECT seq, message, timestamp FROM group_gossip_log WHERE group_id = ? AND seq > ? ORDER BY seq LIMIT ?",
						topic,
						Number(sinceSeq),
						MAX_SEQUENCE_CATCHUP,
					)
		).toArray() as { seq: number; message: ArrayBuffer; timestamp: string }[];
		const messages = [];
		let encodedBytes = 0;
		const payloadBudget = MAX_SEQUENCE_CATCHUP_BYTES - 8 * 1024;
		for (const row of rows) {
			encodedBytes += row.message.byteLength;
			if (encodedBytes > payloadBudget) break;
			messages.push({
				seq: BigInt(row.seq),
				message: fromBinary(GossipEnvelopeSchema, new Uint8Array(row.message)),
				timestamp: row.timestamp,
			});
		}

		console.log(
			`[ws] catchup: lane=${topic.startsWith("group/") ? "group" : "public"} sending=${messages.length}`,
		);
		this.#sendServerMsg(ws, {
			msg: { case: "catchup", value: { topic, messages } },
		});
	}

	#sendChatCatchup(
		ws: WebSocket,
		topic: string,
		chatSv: Record<string, bigint>,
		headIds: Record<string, string>,
		requestedLimit: number,
	) {
		const limit = Math.max(1, Math.min(requestedLimit, MAX_CHAT_CATCHUP));
		const heads = Object.entries(chatSv);
		if (
			heads.length > MAX_CHAT_STATE_WRITERS ||
			Object.keys(headIds).length > MAX_CHAT_STATE_WRITERS ||
			heads.some(([writerId]) => writerId.length > 128 || (headIds[writerId]?.length ?? 0) > 512)
		) {
			this.#sendError(ws, "chat state vector is too large", ErrorCode.MALFORMED);
			return;
		}
		if (heads.some(([, writerSeq]) => writerSeq > BigInt(Number.MAX_SAFE_INTEGER))) {
			this.#sendError(ws, "chat writer sequence exceeds relay range", ErrorCode.MALFORMED);
			return;
		}
		const logicalFloor = Object.values(headIds).reduce((maximum, commitment) => {
			const logicalTime = committedLogicalTime(commitment);
			return logicalTime > maximum ? logicalTime : maximum;
		}, 0n);
		const logicalCeiling = logicalFloor + MAX_REMOTE_LAMPORT_ADVANCE;
		if (logicalCeiling > BigInt(Number.MAX_SAFE_INTEGER)) {
			this.#sendError(ws, "chat logical time exceeds relay range", ErrorCode.MALFORMED);
			return;
		}
		if (!topic.startsWith("festival/")) {
			const rows = this.sql
				.exec(
					"SELECT message FROM group_gossip_log WHERE group_id = ? ORDER BY seq ASC LIMIT ?",
					topic,
					limit,
				)
				.toArray() as { message: ArrayBuffer }[];
			this.#sendChatRowsWithinBudget(ws, topic, rows);
			return;
		}

		const requestId = crypto.randomUUID();
		let rows: { message: ArrayBuffer }[] = [];
		this.ctx.storage.transactionSync(() => {
			for (const [writerId, writerSeq] of heads) {
				this.sql.exec(
					"INSERT INTO chat_catchup_heads(request_id, writer_id, writer_seq, head_id) VALUES (?, ?, ?, ?)",
					requestId,
					writerId,
					Number(writerSeq),
					headIds[writerId] ?? "",
				);
			}
			rows = this.sql
				.exec(
					`SELECT p.message FROM public_gossip_log p
					 LEFT JOIN chat_catchup_heads h
					   ON h.request_id = ? AND h.writer_id = p.writer_id
					 WHERE p.topic = ? AND p.writer_id IS NOT NULL
					   AND p.logical_time <= ?
					   AND p.writer_seq <= COALESCE(h.writer_seq, 0) + ?
					   AND (
					   h.writer_id IS NULL OR p.writer_seq > h.writer_seq OR
					   (p.writer_seq = h.writer_seq AND h.head_id <> '' AND h.head_id <> ?
					    AND (p.message_id || '@' || p.logical_time) <> h.head_id)
					 )
					 ORDER BY p.logical_time ASC, p.writer_id ASC, p.writer_seq ASC, p.message_id ASC
					 LIMIT ?`,
					requestId,
					topic,
					Number(logicalCeiling),
					Number(MAX_REMOTE_LAMPORT_ADVANCE),
					EQUIVOCATED_HEAD_ID,
					limit,
				)
				.toArray() as { message: ArrayBuffer }[];
			this.sql.exec("DELETE FROM chat_catchup_heads WHERE request_id = ?", requestId);
		});
		this.#sendChatRowsWithinBudget(ws, topic, rows);
	}

	#sendChatRowsWithinBudget(ws: WebSocket, topic: string, rows: { message: ArrayBuffer }[]) {
		const messages = rows.map((row) =>
			fromBinary(GossipEnvelopeSchema, new Uint8Array(row.message)),
		);
		while (true) {
			const response = create(RelayServerMessageSchema, {
				msg: { case: "chatDiff", value: { topic, messages } },
			});
			const bytes = toBinary(RelayServerMessageSchema, response);
			if (bytes.byteLength <= MAX_CHAT_CATCHUP_BYTES) {
				ws.send(bytes);
				return;
			}
			if (messages.length === 0) {
				this.#sendError(ws, "Chat catch-up response metadata exceeds limit", ErrorCode.MALFORMED);
				return;
			}
			messages.pop();
		}
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
		let url: URL;
		try {
			url = new URL(request.url);
		} catch {
			return new Response("Invalid request URL", { status: 400 });
		}

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
			this.#publicStateDoc = null;

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

			const expectedDocId = `festival/${this.#festivalId}/state`;
			if (body.docId !== expectedDocId || body.topic !== expectedDocId) {
				return new Response("docId/topic must match the configured festival state", {
					status: 400,
				});
			}

			const updateBytes = base64ToBytes(body.update);
			const envelope = await this.#mutatePublicDoc((doc) => {
				Y.applyUpdate(doc, updateBytes);
			});
			if (envelope.payload.case !== "festivalUpdate") {
				return new Response("Failed to create signed festival update", { status: 500 });
			}
			const festival = envelope.payload.value;
			const signedUpdate = festival.signedUpdate;
			if (!signedUpdate) {
				return new Response("Signed festival update is missing", { status: 500 });
			}

			return Response.json({
				signedUpdate: {
					update: bytesToBase64(signedUpdate.update),
					author: signedUpdate.author,
					signature: bytesToBase64(signedUpdate.signature),
				},
				kind: festival.kind,
				authoritySeq: festival.authoritySeq.toString(),
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

		return new Response(null, { status: 101, webSocket: client });
	}

	async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
		// Only accept binary frames
		if (typeof message === "string") {
			this.#sendError(ws, "Expected binary frame, not text", ErrorCode.MALFORMED);
			return;
		}

		const raw = new Uint8Array(message);
		if (raw.byteLength > MAX_CLIENT_FRAME_BYTES) {
			this.#sendError(ws, "Client frame exceeds relay size limit", ErrorCode.MALFORMED);
			return;
		}
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
			// Session not in memory — happens after hibernation, restore from attachment.
			const rawAtt = ws.deserializeAttachment() as string | null;
			let attachment: {
				topics?: string[];
				authenticated?: boolean;
				publicKey?: string | null;
			} = {};
			if (rawAtt) {
				try {
					attachment = JSON.parse(rawAtt) as typeof attachment;
				} catch {
					this.#sendError(ws, "Invalid session attachment", ErrorCode.MALFORMED);
					return;
				}
			}
			sess = {
				topics: new Set<string>(attachment.topics ?? []),
				authenticated: attachment.authenticated ?? false,
				publicKey: attachment.publicKey ?? null,
			};
			this.#sessions.set(ws, sess);
		}

		const { msg } = parsed;
		let stableChatCheck: ReturnType<typeof checkStableChatRepair> | undefined;

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
				for (const topic of msg.value.topics) {
					sess.topics.add(topic);
				}
				ws.serializeAttachment(
					JSON.stringify({
						topics: [...sess.topics],
						authenticated: sess.authenticated,
						publicKey: sess.publicKey,
					}),
				);
				console.log(`[ws] subscription count: ${sess.topics.size}`);
				this.#sendServerMsg(ws, {
					msg: { case: "subscribed", value: { topics: [...sess.topics] } },
				});
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

				if (envelope.payload.case === "festivalUpdate") {
					this.#sendError(ws, "Clients cannot send festival updates", ErrorCode.UNAUTHORIZED);
					return;
				}

				const envelopeBytes = toBinary(GossipEnvelopeSchema, envelope);
				if (
					(envelope.payload.case === "chat" || envelope.payload.case === "encryptedChat") &&
					envelopeBytes.byteLength > MAX_CHAT_MESSAGE_BYTES
				) {
					this.#sendError(ws, "Chat message exceeds relay size limit", ErrorCode.MALFORMED);
					break;
				}
				let chatMetadata: ChatMetadata | undefined;
				let chatValidation: ChatValidation | undefined;
				if (envelope.payload.case === "chat") {
					chatValidation = this.#validatePublicChat(topic, envelope.payload.value, sess);
					if ("error" in chatValidation) {
						this.#sendError(ws, chatValidation.error, chatValidation.code);
						break;
					}
					chatMetadata = chatValidation.metadata;
				}
				const existingChat = chatMetadata
					? ((
							this.sql
								.exec(
									"SELECT seq, writer_id, writer_seq, logical_time, message FROM public_gossip_log WHERE topic = ? AND message_id = ? LIMIT 1",
									topic,
									chatMetadata.messageId,
								)
								.toArray() as {
								seq: number;
								writer_id: string;
								writer_seq: number;
								logical_time: number;
								message: ArrayBuffer;
							}[]
						)[0] ?? undefined)
					: undefined;
				let broadcastEnvelope = envelope;
				if (existingChat && chatMetadata && envelope.payload.case === "chat") {
					stableChatCheck = checkStableChatRepair(existingChat.message, envelope.payload.value);
					if (!stableChatCheck.valid) {
						this.#sendError(ws, "Chat message ID collision", ErrorCode.MALFORMED);
						break;
					}
					if (!stableChatCheck.repairsFallback) broadcastEnvelope = stableChatCheck.envelope;
				}

				// Receipt lookup, lane writes, and receipt creation form one atomic
				// storage transaction. Exact retries reuse the durable sequence.
				let seq = 0;
				this.ctx.storage.transactionSync(() => {
					seq =
						(
							this.sql
								.exec(
									"SELECT seq FROM relay_receipts WHERE topic = ? AND message = ? LIMIT 1",
									topic,
									envelopeBytes,
								)
								.toArray() as { seq: number }[]
						)[0]?.seq ?? 0;
					if (seq !== 0) return;

					if (existingChat && chatMetadata) {
						seq = existingChat.seq;
						if (stableChatCheck?.repairsFallback) {
							this.sql.exec(
								"UPDATE public_gossip_log SET message = ?, logical_time = ? WHERE seq = ?",
								envelopeBytes,
								chatMetadata.logicalTime,
								seq,
							);
						}
					} else
						switch (envelope.payload.case) {
							case "chat":
								if (!chatMetadata) throw new Error("validated chat metadata missing");
								seq = (
									this.sql
										.exec(
											`INSERT INTO public_gossip_log
										 (topic, message, writer_id, writer_seq, message_id, logical_time)
										 VALUES (?, ?, ?, ?, ?, ?) RETURNING seq`,
											topic,
											envelopeBytes,
											chatMetadata.writerId,
											chatMetadata.writerSeq,
											chatMetadata.messageId,
											chatMetadata.logicalTime,
										)
										.one() as { seq: number }
								).seq;
								break;

							case "encryptedChat":
								seq = (
									this.sql
										.exec(
											"INSERT INTO group_gossip_log (group_id, message) VALUES (?, ?) RETURNING seq",
											topic,
											envelopeBytes,
										)
										.one() as { seq: number }
								).seq;
								break;

							case "groupUpdate":
							case "syncRequest":
							case "syncResponse":
							case "syncUpdate":
								this.sql.exec(
									"INSERT INTO group_yrs_updates (group_id, update_data) VALUES (?, ?)",
									topic,
									envelopeBytes,
								);
								seq = (
									this.sql
										.exec(
											"INSERT INTO group_gossip_log (group_id, message) VALUES (?, ?) RETURNING seq",
											topic,
											envelopeBytes,
										)
										.one() as { seq: number }
								).seq;
								break;

							default:
								seq = (
									this.sql
										.exec(
											"INSERT INTO public_gossip_log (topic, message) VALUES (?, ?) RETURNING seq",
											topic,
											envelopeBytes,
										)
										.one() as { seq: number }
								).seq;
								break;
						}

					this.sql.exec(
						"INSERT INTO relay_receipts (topic, message, seq) VALUES (?, ?, ?)",
						topic,
						envelopeBytes,
						seq,
					);
				});

				// Echo to the sender as a positive persistence acknowledgement,
				// and broadcast to every other subscribed client.
				const broadcastMsg = create(RelayServerMessageSchema, {
					msg: {
						case: "gossip",
						value: {
							topic,
							seq: BigInt(seq),
							message: broadcastEnvelope,
						},
					},
				});
				const broadcastBytes = toBinary(RelayServerMessageSchema, broadcastMsg);

				for (const [other, otherSess] of this.#sessions) {
					if (
						(other === ws && otherSess.topics.has(RELAY_ACK_CAPABILITY_TOPIC)) ||
						(other !== ws && otherSess.topics.has(topic))
					) {
						other.send(broadcastBytes);
					}
				}
				break;
			}

			case "catchup": {
				const { topic, sinceSeq } = msg.value;
				if (topic) this.#sendCatchup(ws, topic, sinceSeq);
				break;
			}

			case "svExchange": {
				const { docId, sv: clientSV } = msg.value;
				if (!docId || clientSV.length === 0) {
					this.#sendError(ws, "sv_exchange requires docId and sv", ErrorCode.MALFORMED);
					break;
				}
				if (docId !== `festival/${this.#festivalId}/state`) {
					this.#sendError(ws, "unknown festival document", ErrorCode.UNAUTHORIZED);
					break;
				}

				// A peer-computed Yrs diff cannot carry festival authority. Return the
				// latest authority-signed full checkpoint through the normal gossip
				// envelope so every client uses the same verification path.
				const checkpoint = await this.#ensureSignedCheckpoint(docId);
				this.#sendServerMsg(ws, {
					msg: {
						case: "gossip",
						value: {
							topic: docId,
							seq: 0n,
							message: checkpoint,
						},
					},
				});
				break;
			}

			case "chatCatchup": {
				const { topic: chatTopic, sv: chatSv, headIds, limit: chatLimit } = msg.value;
				if (!chatTopic) {
					this.#sendError(ws, "chat_catchup requires topic", ErrorCode.MALFORMED);
					break;
				}
				this.#sendChatCatchup(ws, chatTopic, chatSv, headIds, chatLimit || 50);
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
	 * Creates a Yrs doc with root-map keys "stages", "days", "sets" (Y.Maps),
	 * signs the update with the DO's Ed25519 key, and persists to yrs_docs.
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

		// Check if we already have a seeded doc — skip if so
		const existing = this.sql
			.exec("SELECT 1 FROM yrs_docs WHERE doc_id = ? LIMIT 1", docId)
			.toArray();
		if (existing.length > 0) return;

		// Build a Yrs doc using top-level shared maps
		const doc = new Y.Doc();
		const stagesMap = doc.getMap("stages");
		for (const stage of lineup.stages) {
			const m = new Y.Map();
			m.set("name", stage.name);
			m.set("short", stage.short);
			m.set("color", stage.color);
			m.set("order", stage.order);
			stagesMap.set(stage.id, m);
		}
		const daysMap = doc.getMap("days");
		for (const day of lineup.days) {
			const m = new Y.Map();
			m.set("label", day.label);
			m.set("num", day.num);
			m.set("month", day.month);
			m.set("year", day.year);
			daysMap.set(day.id, m);
		}
		const setsMap = doc.getMap("sets");
		for (const set of lineup.sets) {
			const m = new Y.Map();
			m.set("day", set.day);
			m.set("stage", set.stage);
			m.set("artist", set.artist);
			m.set("startMin", set.startMin);
			m.set("durationMin", set.durationMin);
			m.set("genre", set.genre);
			m.set("cancelled", set.cancelled);
			setsMap.set(set.id, m);
		}

		// Assign to in-memory doc and persist
		this.#publicStateDoc = doc;
		const fullState = Y.encodeStateAsUpdate(doc);
		this.sql.exec(
			"INSERT OR REPLACE INTO yrs_docs (doc_id, data, updated_at) VALUES (?, ?, datetime('now'))",
			docId,
			fullState,
		);
		const authoritySeq = this.#nextAuthoritySeq(docId);
		await this.#persistSignedFestivalUpdate(
			docId,
			FestivalUpdateKind.CHECKPOINT,
			authoritySeq,
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
	 * Uses #mutatePublicDoc to load, mutate, sign, persist, and broadcast.
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

		// If there's no existing doc, delegate to seedLineup for genesis
		const existing = this.sql
			.exec("SELECT 1 FROM yrs_docs WHERE doc_id = ? LIMIT 1", docId)
			.toArray();
		if (existing.length === 0) {
			return this.seedLineup(festivalId, lineup);
		}

		await this.#mutatePublicDoc((doc) => {
			const stagesMap = doc.getMap("stages") as Y.Map<Y.Map<unknown>>;
			const daysMap = doc.getMap("days") as Y.Map<Y.Map<unknown>>;
			const setsMap = doc.getMap("sets") as Y.Map<Y.Map<unknown>>;

			// Remove stale entries
			const newStageIds = new Set(lineup.stages.map((s) => s.id));
			for (const key of [...stagesMap.keys()]) {
				if (!newStageIds.has(key)) stagesMap.delete(key);
			}
			const newDayIds = new Set(lineup.days.map((d) => d.id));
			for (const key of [...daysMap.keys()]) {
				if (!newDayIds.has(key)) daysMap.delete(key);
			}
			const newSetIds = new Set(lineup.sets.map((s) => s.id));
			for (const key of [...setsMap.keys()]) {
				if (!newSetIds.has(key)) setsMap.delete(key);
			}

			// Upsert entries
			for (const stage of lineup.stages) {
				let m = stagesMap.get(stage.id);
				if (!m || !(m instanceof Y.Map)) {
					m = new Y.Map();
					stagesMap.set(stage.id, m);
				}
				m.set("name", stage.name);
				m.set("short", stage.short);
				m.set("color", stage.color);
				m.set("order", stage.order);
			}
			for (const day of lineup.days) {
				let m = daysMap.get(day.id);
				if (!m || !(m instanceof Y.Map)) {
					m = new Y.Map();
					daysMap.set(day.id, m);
				}
				m.set("label", day.label);
				m.set("num", day.num);
				m.set("month", day.month);
				m.set("year", day.year);
			}
			for (const set of lineup.sets) {
				let m = setsMap.get(set.id);
				if (!m || !(m instanceof Y.Map)) {
					m = new Y.Map();
					setsMap.set(set.id, m);
				}
				m.set("day", set.day);
				m.set("stage", set.stage);
				m.set("artist", set.artist);
				m.set("startMin", set.startMin);
				m.set("durationMin", set.durationMin);
				m.set("genre", set.genre);
				m.set("cancelled", set.cancelled);
			}
		});

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
		const nowSec = Math.floor(Date.now() / 1000);
		let peerCount = 0;

		await this.#mutatePublicDoc((doc) => {
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

			peerCount = peers.size;
		});

		console.log(
			`[writePeerCheckin] peer ${endpointId.slice(0, 8)}… checked in, ${peerCount} active peers`,
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

		if (!this.#publicStateDoc) return;

		const root = this.#publicStateDoc.getMap("root");
		const peers = root.get("peers") as Y.Map<string> | undefined;
		if (!peers || !(peers instanceof Y.Map) || peers.size === 0) {
			return;
		}

		// Check if any entries are stale before mutating
		const nowSec = Math.floor(Date.now() / 1000);
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

		if (keysToDelete.length === 0) {
			return;
		}

		await this.#mutatePublicDoc((doc) => {
			const r = doc.getMap("root");
			const p = r.get("peers") as Y.Map<string>;
			for (const key of keysToDelete) {
				p.delete(key);
			}
		});

		console.log(
			`[pruneStalePeers] pruned ${keysToDelete.length} stale peers, ${peers.size} remaining`,
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
		if (this.#lat !== null && this.#lon !== null && this.#festivalId) {
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
		const lat = this.#lat;
		const lon = this.#lon;
		if (lat === null || lon === null) {
			throw new Error("Festival coordinates are not configured");
		}
		let forecastDays = 7;
		if (this.#closesAt) {
			const msLeft = new Date(this.#closesAt).getTime() - Date.now();
			forecastDays = Math.max(1, Math.min(7, Math.ceil(msLeft / 86_400_000)));
		}
		const url = `https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}&hourly=temperature_2m,precipitation_probability,weather_code,wind_speed_10m&forecast_days=${forecastDays}&timezone=auto`;
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
			lat,
			lon,
			timezone: data.timezone,
			hourly: data.hourly,
		};
	}

	/** Merge weather into the Yrs doc and broadcast, using #mutatePublicDoc. */
	async #writeWeatherToDoc(weather: WeatherData) {
		await this.#mutatePublicDoc((doc) => {
			const weatherMap = doc.getMap("weather") as Y.Map<Y.Map<unknown>>;
			const hourlyMap = doc.getMap("hourly") as Y.Map<Y.Map<unknown>>;

			// Determine which hourly entries to keep/add
			const nowIso = new Date().toISOString().slice(0, 16);
			const cutoff = this.#closesAt ? this.#closesAt.slice(0, 16) : null;

			// Remove stale future entries (they'll be replaced by fresh forecast)
			for (const key of [...hourlyMap.keys()]) {
				if (key >= nowIso) hourlyMap.delete(key);
			}

			// Add fresh hourly entries
			for (let i = 0; i < weather.hourly.time.length; i++) {
				const t = weather.hourly.time[i];
				if (cutoff && t > cutoff) break;
				const m = new Y.Map();
				m.set("temp", weather.hourly.temperature_2m[i]);
				m.set("precip", weather.hourly.precipitation_probability[i]);
				m.set("code", weather.hourly.weather_code[i]);
				m.set("wind", weather.hourly.wind_speed_10m[i]);
				hourlyMap.set(t, m);
			}

			// Write metadata
			let meta = weatherMap.get("meta");
			if (!meta || !(meta instanceof Y.Map)) {
				meta = new Y.Map();
				weatherMap.set("meta", meta);
			}
			meta.set("updatedAt", weather.updatedAt);
			meta.set("lat", weather.lat);
			meta.set("lon", weather.lon);
			meta.set("timezone", weather.timezone);
		});

		console.log(`[writeWeatherToDoc] wrote weather for festival/${this.#festivalId}/state`);
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
