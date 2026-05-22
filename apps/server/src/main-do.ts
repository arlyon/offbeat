import { DurableObject } from "cloudflare:workers";
import type { ClashfinderSource, Lineup } from "@offbeat/protocol";
import { fetchClashfinder, parseClashfinderApi } from "@offbeat/protocol";
import {
	generateAuthenticationOptions,
	generateRegistrationOptions,
	getExpectedOrigins,
	verifyAuthentication,
	verifyRegistration,
} from "./auth";
import { generateKeypair, sign, verify } from "./signing";

export class MainDO extends DurableObject {
	#publicKey: Uint8Array | null = null;
	#secretKey: Uint8Array | null = null;

	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);

		// Create schema and init keypair
		this.ctx.blockConcurrencyWhile(async () => {
			this.#initSchema();
			await this.#initKeypair();
		});
	}

	async #initKeypair() {
		const stored = (await this.ctx.storage.get("ed25519_secret_key")) as Uint8Array | undefined;
		if (stored) {
			this.#secretKey = stored;
			this.#publicKey = (await this.ctx.storage.get("ed25519_public_key")) as Uint8Array;
		} else {
			const { publicKey, secretKey } = generateKeypair();
			this.#publicKey = publicKey;
			this.#secretKey = secretKey;
			await this.ctx.storage.put("ed25519_secret_key", secretKey);
			await this.ctx.storage.put("ed25519_public_key", publicKey);
		}
	}

	#initSchema() {
		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS festivals (
				id TEXT PRIMARY KEY,
				name TEXT NOT NULL,
				year INTEGER NOT NULL,
				location TEXT NOT NULL,
				city TEXT NOT NULL,
				country TEXT NOT NULL,
				start_date TEXT NOT NULL,
				end_date TEXT NOT NULL,
				genres TEXT NOT NULL DEFAULT '[]',
				status TEXT NOT NULL DEFAULT 'upcoming',
				clashfinder_id TEXT,
				public_key TEXT,
				lat REAL,
				lon REAL,
				updated_at TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE TABLE IF NOT EXISTS festival_stages (
				id TEXT PRIMARY KEY,
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				name TEXT NOT NULL,
				short TEXT NOT NULL,
				color TEXT NOT NULL,
				sort_order INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS festival_days (
				id TEXT NOT NULL,
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				label TEXT NOT NULL,
				num INTEGER NOT NULL,
				month TEXT NOT NULL,
				year INTEGER NOT NULL,
				PRIMARY KEY (festival_id, id)
			);

			CREATE TABLE IF NOT EXISTS festival_sets (
				id TEXT NOT NULL,
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				day_id TEXT NOT NULL,
				stage_id TEXT NOT NULL,
				artist TEXT NOT NULL,
				start_min INTEGER NOT NULL,
				duration_min INTEGER NOT NULL,
				genre TEXT NOT NULL DEFAULT '',
				cancelled INTEGER NOT NULL DEFAULT 0,
				PRIMARY KEY (festival_id, id)
			);

		`);

		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS credentials (
				id TEXT PRIMARY KEY,
				user_id TEXT NOT NULL,
				public_key TEXT NOT NULL,
				credential_data TEXT NOT NULL,
				created_at TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE INDEX IF NOT EXISTS idx_creds_user ON credentials(user_id);

			CREATE TABLE IF NOT EXISTS admins (
				public_key TEXT PRIMARY KEY
			);

			CREATE TABLE IF NOT EXISTS pending_admins (
				public_key TEXT PRIMARY KEY,
				display_name TEXT NOT NULL DEFAULT '',
				requested_at TEXT NOT NULL DEFAULT (datetime('now'))
			);

			CREATE TABLE IF NOT EXISTS revocations (
				public_key TEXT PRIMARY KEY,
				revoked_at TEXT NOT NULL DEFAULT (datetime('now')),
				reason TEXT
			);
		`);
	}

	/** Insert or replace stages, days, and sets for a festival. */
	#upsertLineup(festivalId: string, lineup: Lineup) {
		// Clear existing lineup data for this festival
		this.sql.exec("DELETE FROM festival_sets WHERE festival_id = ?", festivalId);
		this.sql.exec("DELETE FROM festival_days WHERE festival_id = ?", festivalId);
		this.sql.exec("DELETE FROM festival_stages WHERE festival_id = ?", festivalId);

		for (const stage of lineup.stages) {
			this.sql.exec(
				`INSERT INTO festival_stages (id, festival_id, name, short, color, sort_order)
				 VALUES (?, ?, ?, ?, ?, ?)`,
				stage.id,
				festivalId,
				stage.name,
				stage.short,
				stage.color,
				stage.order,
			);
		}

		for (const day of lineup.days) {
			this.sql.exec(
				`INSERT INTO festival_days (id, festival_id, label, num, month, year)
				 VALUES (?, ?, ?, ?, ?, ?)`,
				day.id,
				festivalId,
				day.label,
				day.num,
				day.month,
				day.year,
			);
		}

		for (const set of lineup.sets) {
			this.sql.exec(
				`INSERT INTO festival_sets (id, festival_id, day_id, stage_id, artist, start_min, duration_min, genre, cancelled)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
				set.id,
				festivalId,
				set.day,
				set.stage,
				set.artist,
				set.startMin,
				set.durationMin,
				set.genre,
				set.cancelled ? 1 : 0,
			);
		}
	}

	#getFestivals() {
		const rows = this.sql.exec("SELECT * FROM festivals ORDER BY start_date").toArray() as Record<
			string,
			unknown
		>[];

		return rows.map((row) => {
			const stages = this.sql
				.exec("SELECT * FROM festival_stages WHERE festival_id = ? ORDER BY sort_order", row.id)
				.toArray() as Record<string, unknown>[];

			return {
				id: row.id,
				name: row.name,
				year: row.year,
				location: row.location,
				city: row.city,
				country: row.country,
				startDate: row.start_date,
				endDate: row.end_date,
				genres: JSON.parse(row.genres as string),
				status: row.status,
				clashfinderId: row.clashfinder_id ?? undefined,
				publicKey: row.public_key ?? "",
				updatedAt: row.updated_at,
				lat: (row.lat as number) ?? undefined,
				lon: (row.lon as number) ?? undefined,
				stages: stages.map((s) => ({
					id: s.id,
					name: s.name,
					short: s.short,
					color: s.color,
					order: s.sort_order,
				})),
			};
		});
	}

	#getFestival(id: string) {
		const rows = this.sql.exec("SELECT * FROM festivals WHERE id = ?", id).toArray() as Record<
			string,
			unknown
		>[];
		const row = rows[0] ?? null;

		if (!row) return null;

		const stages = this.sql
			.exec("SELECT * FROM festival_stages WHERE festival_id = ? ORDER BY sort_order", id)
			.toArray() as Record<string, unknown>[];

		return {
			id: row.id,
			name: row.name,
			year: row.year,
			location: row.location,
			city: row.city,
			country: row.country,
			startDate: row.start_date,
			endDate: row.end_date,
			genres: JSON.parse(row.genres as string),
			status: row.status,
			clashfinderId: row.clashfinder_id ?? undefined,
			publicKey: row.public_key ?? "",
			updatedAt: row.updated_at,
			lat: (row.lat as number) ?? undefined,
			lon: (row.lon as number) ?? undefined,
			stages: stages.map((s) => ({
				id: s.id,
				name: s.name,
				short: s.short,
				color: s.color,
				order: s.sort_order,
			})),
		};
	}

	/** Issue a signed attestation binding an Ed25519 public key to a WebAuthn registration. */
	async #issueAttestation(ed25519PubkeyHex: string): Promise<{
		message: string;
		signature: string;
		issuer: string;
	}> {
		if (!this.#secretKey || !this.#publicKey) {
			throw new Error("MainDO keypair not initialized");
		}
		const issuedAt = Math.floor(Date.now() / 1000);
		const expiresAt = issuedAt + 30 * 24 * 60 * 60; // 30 days
		const message = `attestation:v1:${ed25519PubkeyHex}:${issuedAt}:${expiresAt}`;
		const sig = await sign(this.#secretKey, new TextEncoder().encode(message));
		return {
			message,
			signature: bytesToHex(sig),
			issuer: bytesToHex(this.#publicKey),
		};
	}

	/**
	 * Verify admin auth from request headers.
	 * Expects `X-Admin-Key` (hex public key) and `X-Admin-Sig` (hex signature
	 * over the request path). Returns null on success, or an error Response.
	 */
	async #requireAdmin(request: Request): Promise<Response | null> {
		const pubKeyHex = request.headers.get("X-Admin-Key");
		const sigHex = request.headers.get("X-Admin-Sig");
		if (!pubKeyHex || !sigHex) {
			return new Response("X-Admin-Key and X-Admin-Sig headers required", { status: 401 });
		}

		const isAdmin =
			this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", pubKeyHex).toArray().length > 0;
		if (!isAdmin) {
			return new Response("Not an admin", { status: 403 });
		}

		const url = new URL(request.url);
		const message = new TextEncoder().encode(url.pathname);
		const valid = await verify(hexToBytes(pubKeyHex), message, hexToBytes(sigHex));
		if (!valid) {
			return new Response("Invalid signature", { status: 401 });
		}

		return null;
	}

	/** Look up a stored WebAuthn credential by its credential ID. */
	#getCredentialById(credentialId: string): {
		id: string;
		publicKey: Uint8Array;
		counter: number;
		transports?: string[];
	} | null {
		const rows = this.sql
			.exec("SELECT credential_data FROM credentials WHERE id = ?", credentialId)
			.toArray() as { credential_data: string }[];
		if (rows.length === 0) return null;
		const data = JSON.parse(rows[0].credential_data) as {
			credentialId: string;
			publicKey: Record<string, number>;
			counter: number;
			transports?: string[];
		};
		// publicKey was stored as a serialized Uint8Array (JSON object with numeric keys)
		const pkBytes = new Uint8Array(Object.values(data.publicKey));
		return {
			id: data.credentialId,
			publicKey: pkBytes,
			counter: data.counter ?? 0,
			transports: data.transports,
		};
	}

	#getLineup(id: string): Lineup | null {
		const festival = this.#getFestival(id);
		if (!festival) return null;

		interface StageRow {
			id: string;
			name: string;
			short: string;
			color: string;
			sort_order: number;
		}
		interface DayRow {
			id: string;
			label: string;
			num: number;
			month: string;
			year: number;
		}
		interface SetRow {
			id: string;
			day_id: string;
			stage_id: string;
			artist: string;
			start_min: number;
			duration_min: number;
			genre: string;
			cancelled: number;
		}

		const stages = this.sql
			.exec("SELECT * FROM festival_stages WHERE festival_id = ? ORDER BY sort_order", id)
			.toArray() as unknown as StageRow[];

		const days = this.sql
			.exec("SELECT * FROM festival_days WHERE festival_id = ? ORDER BY num", id)
			.toArray() as unknown as DayRow[];

		const sets = this.sql
			.exec("SELECT * FROM festival_sets WHERE festival_id = ? ORDER BY start_min", id)
			.toArray() as unknown as SetRow[];

		return {
			festival: { id, name: festival.name as string, location: festival.location as string },
			stages: stages.map((s) => ({
				id: s.id,
				name: s.name,
				short: s.short,
				color: s.color,
				order: s.sort_order,
			})),
			days: days.map((d) => ({
				id: d.id,
				label: d.label,
				num: d.num,
				month: d.month,
				year: d.year,
			})),
			sets: sets.map((s) => ({
				id: s.id,
				day: s.day_id,
				stage: s.stage_id,
				artist: s.artist,
				startMin: s.start_min,
				durationMin: s.duration_min,
				genre: s.genre,
				cancelled: s.cancelled === 1,
			})),
		};
	}

	/** Clean up expired challenges. */
	async alarm() {
		// Delete all expired challenges (older than 5 minutes)
		const keys = await this.ctx.storage.list({ prefix: "challenge:" });
		if (keys.size > 0) {
			await this.ctx.storage.delete([...keys.keys()]);
		}
	}

	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);
		const path = url.pathname;
		const method = request.method;
		const env = this.env as Record<string, unknown>;

		// GET /festivals
		if (method === "GET" && path === "/festivals") {
			return Response.json(this.#getFestivals());
		}

		// GET /festivals/:id/lineup
		const lineupMatch = path.match(/^\/festivals\/([^/]+)\/lineup$/);
		if (method === "GET" && lineupMatch) {
			const id = lineupMatch[1];
			const lineup = this.#getLineup(id);
			if (!lineup) return new Response("Festival not found", { status: 404 });
			return Response.json(lineup);
		}

		// GET /festivals/:id
		const festivalMatch = path.match(/^\/festivals\/([^/]+)$/);
		if (method === "GET" && festivalMatch) {
			const id = festivalMatch[1];
			const festival = this.#getFestival(id);
			if (!festival) return new Response("Festival not found", { status: 404 });
			return Response.json(festival);
		}

		// POST /festivals — create a new festival from Clashfinder (admin-only).
		// Expects: { source: { festivalId, clashfinderId, name, location, city, country, genres } }
		// Fetches lineup from Clashfinder API and stores it.
		if (method === "POST" && path === "/festivals") {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;

			const body = (await request.json()) as {
				source?: ClashfinderSource;
			};

			if (!body.source) {
				return new Response("source is required", { status: 400 });
			}

			const src = body.source;
			if (!src.festivalId || !src.clashfinderId || !src.name) {
				return new Response("source requires festivalId, clashfinderId, name", {
					status: 400,
				});
			}

			if (this.#getFestival(src.festivalId)) {
				return new Response("Festival already exists", { status: 409 });
			}

			// Fetch lineup from Clashfinder API
			const cfUsername = (env as Record<string, string>).CLASHFINDER_USERNAME;
			const cfKey = (env as Record<string, string>).CLASHFINDER_PRIVATE_KEY;
			if (!cfUsername || !cfKey) {
				return new Response("Clashfinder credentials not configured", { status: 500 });
			}

			const apiResponse = await fetchClashfinder(src.clashfinderId, {
				username: cfUsername,
				privateKey: cfKey,
			});
			const lineup = parseClashfinderApi(src.festivalId, apiResponse, {
				name: src.name,
				location: src.location,
			});

			// Derive start/end dates from the API event datetimes
			const allStarts = apiResponse.locations.flatMap((loc) => loc.events.map((e) => e.start));
			const allEnds = apiResponse.locations.flatMap((loc) => loc.events.map((e) => e.end));
			const allDatetimes = [...allStarts, ...allEnds]
				.map((dt) => new Date(dt.replace(" ", "T")))
				.filter((d) => !Number.isNaN(d.getTime()));
			allDatetimes.sort((a, b) => a.getTime() - b.getTime());
			const startDate =
				allDatetimes[0]?.toISOString().split("T")[0] ?? new Date().toISOString().split("T")[0];
			const endDate =
				allDatetimes[allDatetimes.length - 1]?.toISOString().split("T")[0] ?? startDate;
			const year = allDatetimes[0]?.getFullYear() ?? new Date().getFullYear();

			this.sql.exec(
				`INSERT INTO festivals (id, name, year, location, city, country, start_date, end_date, genres, status, clashfinder_id, lat, lon)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
				src.festivalId,
				src.name,
				year,
				src.location ?? "",
				src.city ?? "",
				src.country ?? "",
				startDate,
				endDate,
				JSON.stringify(src.genres ?? []),
				"upcoming",
				src.clashfinderId,
				src.lat ?? null,
				src.lon ?? null,
			);

			this.#upsertLineup(src.festivalId, lineup);

			return Response.json(
				{ festival: this.#getFestival(src.festivalId), lineup: this.#getLineup(src.festivalId) },
				{ status: 201 },
			);
		}

		// PUT /festivals/:id — update festival metadata (admin-only)
		const festivalPutMatch = path.match(/^\/festivals\/([^/]+)$/);
		if (method === "PUT" && festivalPutMatch) {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;

			const id = festivalPutMatch[1];
			if (!this.#getFestival(id)) {
				return new Response("Festival not found", { status: 404 });
			}

			const body = (await request.json()) as Record<string, unknown>;
			const updates: string[] = [];
			const values: unknown[] = [];

			for (const [key, col] of [
				["name", "name"],
				["year", "year"],
				["location", "location"],
				["city", "city"],
				["country", "country"],
				["startDate", "start_date"],
				["endDate", "end_date"],
				["status", "status"],
			]) {
				if (body[key] !== undefined) {
					updates.push(`${col} = ?`);
					values.push(body[key]);
				}
			}
			if (body.genres !== undefined) {
				updates.push("genres = ?");
				values.push(JSON.stringify(body.genres));
			}
			for (const [key, col] of [
				["lat", "lat"],
				["lon", "lon"],
			]) {
				if (body[key] !== undefined) {
					updates.push(`${col} = ?`);
					values.push(body[key]);
				}
			}

			if (updates.length > 0) {
				updates.push("updated_at = datetime('now')");
				values.push(id);
				this.sql.exec(`UPDATE festivals SET ${updates.join(", ")} WHERE id = ?`, ...values);
			}

			return Response.json(this.#getFestival(id));
		}

		// DELETE /festivals/:id/reset — admin auth gate for Festival DO reset
		const festivalResetMatch = path.match(/^\/festivals\/([^/]+)\/reset$/);
		if (method === "DELETE" && festivalResetMatch) {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;
			return Response.json({ ok: true });
		}

		// DELETE /festivals/:id — delete a festival (admin-only)
		const festivalDeleteMatch = path.match(/^\/festivals\/([^/]+)$/);
		if (method === "DELETE" && festivalDeleteMatch) {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;

			const id = festivalDeleteMatch[1];
			if (!this.#getFestival(id)) {
				return new Response("Festival not found", { status: 404 });
			}

			// Delete lineup data first (foreign key constraints)
			this.sql.exec("DELETE FROM festival_sets WHERE festival_id = ?", id);
			this.sql.exec("DELETE FROM festival_days WHERE festival_id = ?", id);
			this.sql.exec("DELETE FROM festival_stages WHERE festival_id = ?", id);
			// Delete the festival
			this.sql.exec("DELETE FROM festivals WHERE id = ?", id);

			return new Response(null, { status: 204 });
		}

		// PUT /festivals/:id/lineup — refresh lineup from Clashfinder (admin-only)
		const lineupPutMatch = path.match(/^\/festivals\/([^/]+)\/lineup$/);
		if (method === "PUT" && lineupPutMatch) {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;

			const id = lineupPutMatch[1];
			const festival = this.#getFestival(id);
			if (!festival) {
				return new Response("Festival not found", { status: 404 });
			}

			const clashfinderId = (festival as Record<string, unknown>).clashfinderId as
				| string
				| undefined;
			if (!clashfinderId) {
				return new Response("Festival has no clashfinder_id configured", { status: 400 });
			}

			// Fetch updated lineup from Clashfinder API
			const cfUsername = (env as Record<string, string>).CLASHFINDER_USERNAME;
			const cfKey = (env as Record<string, string>).CLASHFINDER_PRIVATE_KEY;
			if (!cfUsername || !cfKey) {
				return new Response("Clashfinder credentials not configured", { status: 500 });
			}

			const apiResponse = await fetchClashfinder(clashfinderId, {
				username: cfUsername,
				privateKey: cfKey,
			});
			const lineup = parseClashfinderApi(id, apiResponse, {
				name: festival.name as string,
				location: festival.location as string,
			});

			this.#upsertLineup(id, lineup);

			return Response.json(this.#getLineup(id));
		}

		// GET /auth/public-key — MainDO's Ed25519 public key (attestation issuer)
		if (method === "GET" && path === "/auth/public-key") {
			if (!this.#publicKey) {
				return new Response("Key not initialized", { status: 500 });
			}
			return new Response(bytesToHex(this.#publicKey), {
				headers: { "Content-Type": "text/plain" },
			});
		}

		// POST /auth/register/begin
		if (method === "POST" && path === "/auth/register/begin") {
			const body = (await request.json()) as { userId: string };
			const options = await generateRegistrationOptions(body.userId, env);
			console.log(
				"Register begin — RP_ID:",
				env.RP_ID,
				"options.rp:",
				(options as Record<string, unknown>).rp,
				"expectedOrigins:",
				getExpectedOrigins(env),
			);
			// Store challenge for verification (5-min TTL)
			const challenge = (options as { challenge: string }).challenge;
			await this.ctx.storage.put(`challenge:${challenge}`, body.userId);
			await this.ctx.storage.setAlarm(Date.now() + 5 * 60 * 1000);
			return Response.json(options);
		}

		// POST /auth/register/complete
		if (method === "POST" && path === "/auth/register/complete") {
			const body = (await request.json()) as {
				webauthnResponse: unknown;
				challenge: string;
				ed25519PublicKey: string;
			};
			console.log("Register complete — webauthnResponse:", JSON.stringify(body.webauthnResponse));
			if (!body.ed25519PublicKey || body.ed25519PublicKey.length !== 64) {
				return new Response("ed25519PublicKey must be 64 hex chars", { status: 400 });
			}
			if (!body.challenge) {
				return new Response("challenge is required", { status: 400 });
			}

			// Verify the challenge was issued by us
			const storedUserId = await this.ctx.storage.get<string>(`challenge:${body.challenge}`);
			if (!storedUserId) {
				return new Response("Invalid or expired challenge", { status: 400 });
			}
			await this.ctx.storage.delete(`challenge:${body.challenge}`);

			// Dev bypass: skip WebAuthn verification when DEV_BYPASS_WEBAUTHN is set
			// This allows integration tests to run without real WebAuthn credentials
			const devBypass = env.DEV_BYPASS_WEBAUTHN === "true";

			let result: Awaited<ReturnType<typeof verifyRegistration>>;
			if (devBypass) {
				// In dev mode, accept any Ed25519 key without WebAuthn verification
				result = { verified: true, credentialId: crypto.randomUUID() };
			} else {
				try {
					result = await verifyRegistration(body.webauthnResponse, body.challenge, env);
				} catch (err) {
					console.error("Registration verification threw:", err);
					console.error("RP_ID:", env.RP_ID, "Expected origins:", getExpectedOrigins(env));
					return new Response(`Registration verification failed: ${err}`, { status: 400 });
				}
				if (!result.verified) {
					console.error("Registration verification returned verified=false");
					return new Response("Registration verification failed: not verified", { status: 400 });
				}
			}

			// Store WebAuthn credential with the Ed25519 public key
			this.sql.exec(
				`INSERT INTO credentials (id, user_id, public_key, credential_data, created_at)
				 VALUES (?, ?, ?, ?, datetime('now'))`,
				result.credentialId ?? crypto.randomUUID(),
				body.ed25519PublicKey,
				body.ed25519PublicKey,
				JSON.stringify(result),
			);
			const attestation = await this.#issueAttestation(body.ed25519PublicKey);
			return Response.json({ attestation });
		}

		// POST /auth/recover/begin — new device recovery
		if (method === "POST" && path === "/auth/recover/begin") {
			const options = await generateAuthenticationOptions(env);
			const challenge = (options as { challenge: string }).challenge;
			await this.ctx.storage.put(`challenge:${challenge}`, "recovery");
			await this.ctx.storage.setAlarm(Date.now() + 5 * 60 * 1000);
			return Response.json(options);
		}

		// POST /auth/recover/complete — verify assertion, confirm Ed25519 key matches
		if (method === "POST" && path === "/auth/recover/complete") {
			const body = (await request.json()) as {
				assertion: unknown;
				challenge: string;
				ed25519PublicKey: string;
			};
			if (!body.ed25519PublicKey || body.ed25519PublicKey.length !== 64) {
				return new Response("ed25519PublicKey must be 64 hex chars", { status: 400 });
			}
			if (!body.challenge) {
				return new Response("challenge is required", { status: 400 });
			}

			// Verify the challenge was issued by us
			const stored = await this.ctx.storage.get<string>(`challenge:${body.challenge}`);
			if (!stored) {
				return new Response("Invalid or expired challenge", { status: 400 });
			}
			await this.ctx.storage.delete(`challenge:${body.challenge}`);

			// Look up the credential by the assertion's credential ID
			const assertionObj = body.assertion as { id?: string };
			const credentialId = assertionObj?.id;
			if (!credentialId) {
				return new Response("assertion.id is required", { status: 400 });
			}

			const credential = this.#getCredentialById(credentialId);
			if (!credential) {
				return new Response("Unknown credential", { status: 403 });
			}

			let result: Awaited<ReturnType<typeof verifyAuthentication>>;
			try {
				result = await verifyAuthentication(
					body.assertion,
					body.challenge,
					{
						id: credential.id,
						publicKey: credential.publicKey as Uint8Array<ArrayBuffer>,
						counter: credential.counter,
						transports: credential.transports as
							| import("@simplewebauthn/server").AuthenticatorTransportFuture[]
							| undefined,
					},
					env,
				);
			} catch (err) {
				console.error("Recovery authentication threw:", err);
				console.error("RP_ID:", env.RP_ID, "Expected origins:", getExpectedOrigins(env));
				return new Response(`Authentication failed: ${err}`, { status: 400 });
			}
			if (!result.verified) {
				console.error("Recovery authentication returned verified=false");
				return new Response("Authentication failed: not verified", { status: 400 });
			}

			// Verify the Ed25519 key matches what we stored at registration
			const storedCred = this.sql
				.exec("SELECT public_key FROM credentials WHERE public_key = ?", body.ed25519PublicKey)
				.toArray();
			if (storedCred.length === 0) {
				return new Response("Ed25519 key does not match registered key", { status: 403 });
			}
			const attestation = await this.#issueAttestation(body.ed25519PublicKey);
			return Response.json({ attestation });
		}

		// POST /auth/refresh — re-issue attestation for existing credential
		if (method === "POST" && path === "/auth/refresh") {
			const body = (await request.json()) as {
				assertion: unknown;
				challenge: string;
				ed25519PublicKey: string;
			};
			if (!body.ed25519PublicKey || body.ed25519PublicKey.length !== 64) {
				return new Response("ed25519PublicKey must be 64 hex chars", { status: 400 });
			}
			if (!body.challenge) {
				return new Response("challenge is required", { status: 400 });
			}

			const storedChallenge = await this.ctx.storage.get<string>(`challenge:${body.challenge}`);
			if (!storedChallenge) {
				return new Response("Invalid or expired challenge", { status: 400 });
			}
			await this.ctx.storage.delete(`challenge:${body.challenge}`);

			const assertionObj = body.assertion as { id?: string };
			const credentialId = assertionObj?.id;
			if (!credentialId) {
				return new Response("assertion.id is required", { status: 400 });
			}

			const credential = this.#getCredentialById(credentialId);
			if (!credential) {
				return new Response("Unknown credential", { status: 403 });
			}

			let result: Awaited<ReturnType<typeof verifyAuthentication>>;
			try {
				result = await verifyAuthentication(
					body.assertion,
					body.challenge,
					{
						id: credential.id,
						publicKey: credential.publicKey as Uint8Array<ArrayBuffer>,
						counter: credential.counter,
						transports: credential.transports as
							| import("@simplewebauthn/server").AuthenticatorTransportFuture[]
							| undefined,
					},
					env,
				);
			} catch (err) {
				console.error("Refresh authentication threw:", err);
				console.error("RP_ID:", env.RP_ID, "Expected origins:", getExpectedOrigins(env));
				return new Response(`Authentication failed: ${err}`, { status: 400 });
			}
			if (!result.verified) {
				console.error("Refresh authentication returned verified=false");
				return new Response("Authentication failed: not verified", { status: 400 });
			}

			// Check key is registered and not revoked
			const storedCred = this.sql
				.exec("SELECT public_key FROM credentials WHERE public_key = ?", body.ed25519PublicKey)
				.toArray();
			if (storedCred.length === 0) {
				return new Response("Unknown key", { status: 403 });
			}
			const revoked = this.sql
				.exec("SELECT 1 FROM revocations WHERE public_key = ?", body.ed25519PublicKey)
				.toArray();
			if (revoked.length > 0) {
				return new Response("Key revoked", { status: 423 });
			}
			const attestation = await this.#issueAttestation(body.ed25519PublicKey);
			return Response.json({ attestation });
		}

		// PUT /admins — register a global admin public key.
		// First admin is auto-accepted (bootstrap). Subsequent require existing admin auth.
		if (method === "PUT" && path === "/admins") {
			const body = (await request.json()) as {
				publicKey: string;
			};
			if (!body.publicKey || body.publicKey.length !== 64) {
				return new Response("publicKey must be 64 hex chars", {
					status: 400,
				});
			}

			const count = (this.sql.exec("SELECT COUNT(*) as cnt FROM admins").one() as { cnt: number })
				.cnt;

			if (count > 0) {
				// Require existing admin to promote
				const authResult = await this.#requireAdmin(request);
				if (authResult instanceof Response) return authResult;
			}

			this.sql.exec("INSERT OR IGNORE INTO admins (public_key) VALUES (?)", body.publicKey);
			// Remove from pending if they were there
			this.sql.exec("DELETE FROM pending_admins WHERE public_key = ?", body.publicKey);
			return Response.json({ ok: true });
		}

		// GET /admins — list all global admin public keys
		if (method === "GET" && path === "/admins") {
			const rows = this.sql.exec("SELECT public_key FROM admins").toArray() as {
				public_key: string;
			}[];
			return Response.json(rows.map((r) => r.public_key));
		}

		// POST /admins/request — request to become an admin
		if (method === "POST" && path === "/admins/request") {
			const body = (await request.json()) as {
				publicKey: string;
				displayName?: string;
			};
			if (!body.publicKey || body.publicKey.length !== 64) {
				return new Response("publicKey must be 64 hex chars", { status: 400 });
			}

			// Already an admin?
			const isAdmin =
				this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", body.publicKey).toArray()
					.length > 0;
			if (isAdmin) {
				return Response.json({ status: "already_admin" });
			}

			this.sql.exec(
				`INSERT OR REPLACE INTO pending_admins (public_key, display_name) VALUES (?, ?)`,
				body.publicKey,
				body.displayName ?? "",
			);
			return Response.json({ status: "pending" });
		}

		// GET /admins/requests — list pending admin requests
		if (method === "GET" && path === "/admins/requests") {
			const rows = this.sql
				.exec(
					"SELECT public_key, display_name, requested_at FROM pending_admins ORDER BY requested_at",
				)
				.toArray() as { public_key: string; display_name: string; requested_at: string }[];
			return Response.json(
				rows.map((r) => ({
					publicKey: r.public_key,
					displayName: r.display_name,
					requestedAt: r.requested_at,
				})),
			);
		}

		// POST /admins/requests/:key/approve — approve a pending admin request
		const approveMatch = path.match(/^\/admins\/requests\/([0-9a-f]{64})\/approve$/);
		if (method === "POST" && approveMatch) {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;

			const key = approveMatch[1];
			const pending = this.sql
				.exec("SELECT 1 FROM pending_admins WHERE public_key = ?", key)
				.toArray();
			if (pending.length === 0) {
				return new Response("No pending request for this key", { status: 404 });
			}

			this.sql.exec("INSERT OR IGNORE INTO admins (public_key) VALUES (?)", key);
			this.sql.exec("DELETE FROM pending_admins WHERE public_key = ?", key);
			return Response.json({ ok: true });
		}

		// POST /admins/requests/:key/deny — deny a pending admin request
		const denyMatch = path.match(/^\/admins\/requests\/([0-9a-f]{64})\/deny$/);
		if (method === "POST" && denyMatch) {
			const authResult = await this.#requireAdmin(request);
			if (authResult instanceof Response) return authResult;

			const key = denyMatch[1];
			this.sql.exec("DELETE FROM pending_admins WHERE public_key = ?", key);
			return Response.json({ ok: true });
		}

		return new Response("Not found", { status: 404 });
	}
}

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
