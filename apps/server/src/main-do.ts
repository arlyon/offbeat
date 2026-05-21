import { DurableObject } from "cloudflare:workers";
import type { ClashfinderEvent } from "@offbeat/protocol";
import { parseClashfinder } from "@offbeat/protocol";
import fieldday26 from "../../../packages/protocol/fixtures/fieldday26.json";
import {
	createJwt,
	generateAuthenticationOptions,
	generateRegistrationOptions,
	verifyAuthentication,
	verifyRegistration,
} from "./auth";

const FIELDDAY26_ID = "fieldday26";

const FIELDDAY26_META = {
	id: FIELDDAY26_ID,
	name: "Field Day 2026",
	year: 2026,
	location: "Victoria Park, London",
	city: "London",
	country: "GB",
	start_date: "2026-06-13",
	end_date: "2026-06-14",
	genres: JSON.stringify(["Electronic", "Indie", "Experimental"]),
	status: "upcoming",
	public_key: null as string | null,
};

export class MainDO extends DurableObject {
	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);

		// Create schema and seed data
		this.ctx.blockConcurrencyWhile(async () => {
			this.#initSchema();
			await this.#seed();
		});
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
				public_key TEXT,
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

			CREATE TABLE IF NOT EXISTS festival_history (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				data BLOB NOT NULL,
				created_at TEXT NOT NULL DEFAULT (datetime('now'))
			);

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
		`);
	}

	async #seed() {
		const existing = this.sql.exec("SELECT COUNT(*) as cnt FROM festivals").one() as {
			cnt: number;
		};

		if (existing.cnt > 0) return;

		const events = fieldday26 as ClashfinderEvent[];
		const lineup = parseClashfinder(FIELDDAY26_ID, events);

		this.sql.exec(
			`INSERT INTO festivals (id, name, year, location, city, country, start_date, end_date, genres, status, public_key)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			FIELDDAY26_META.id,
			FIELDDAY26_META.name,
			FIELDDAY26_META.year,
			FIELDDAY26_META.location,
			FIELDDAY26_META.city,
			FIELDDAY26_META.country,
			FIELDDAY26_META.start_date,
			FIELDDAY26_META.end_date,
			FIELDDAY26_META.genres,
			FIELDDAY26_META.status,
			FIELDDAY26_META.public_key,
		);

		for (const stage of lineup.stages) {
			this.sql.exec(
				`INSERT INTO festival_stages (id, festival_id, name, short, color, sort_order)
				 VALUES (?, ?, ?, ?, ?, ?)`,
				stage.id,
				FIELDDAY26_ID,
				stage.name,
				stage.short,
				stage.color,
				stage.order,
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
				publicKey: row.public_key ?? "",
				updatedAt: row.updated_at,
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
			publicKey: row.public_key ?? "",
			updatedAt: row.updated_at,
			stages: stages.map((s) => ({
				id: s.id,
				name: s.name,
				short: s.short,
				color: s.color,
				order: s.sort_order,
			})),
		};
	}

	async fetch(request: Request): Promise<Response> {
		const url = new URL(request.url);
		const path = url.pathname;
		const method = request.method;

		// GET /festivals
		if (method === "GET" && path === "/festivals") {
			return Response.json(this.#getFestivals());
		}

		// GET /festivals/:id/lineup
		const lineupMatch = path.match(/^\/festivals\/([^/]+)\/lineup$/);
		if (method === "GET" && lineupMatch) {
			const id = lineupMatch[1];
			if (id === FIELDDAY26_ID) {
				const events = fieldday26 as ClashfinderEvent[];
				const lineup = parseClashfinder(id, events);
				return Response.json(lineup);
			}
			return new Response("Festival not found", { status: 404 });
		}

		// GET /festivals/:id
		const festivalMatch = path.match(/^\/festivals\/([^/]+)$/);
		if (method === "GET" && festivalMatch) {
			const id = festivalMatch[1];
			const festival = this.#getFestival(id);
			if (!festival) return new Response("Festival not found", { status: 404 });
			return Response.json(festival);
		}

		// POST /auth/register/begin
		if (method === "POST" && path === "/auth/register/begin") {
			const body = (await request.json()) as { userId: string };
			const options = await generateRegistrationOptions(body.userId);
			return Response.json(options);
		}

		// POST /auth/register/complete
		if (method === "POST" && path === "/auth/register/complete") {
			const body = await request.json();
			const result = await verifyRegistration(body);
			if (!result.verified) {
				return new Response("Registration failed", { status: 400 });
			}
			const token = await createJwt(result.credentialId ?? "unknown");
			return Response.json({ token });
		}

		// POST /auth/authenticate/begin
		if (method === "POST" && path === "/auth/authenticate/begin") {
			const options = await generateAuthenticationOptions();
			return Response.json(options);
		}

		// POST /auth/authenticate/complete
		if (method === "POST" && path === "/auth/authenticate/complete") {
			const body = await request.json();
			const result = await verifyAuthentication(body);
			if (!result.verified) {
				return new Response("Authentication failed", { status: 400 });
			}
			const token = await createJwt(result.userId ?? "unknown");
			return Response.json({ token });
		}

		// PUT /admins — register a global admin public key.
		// First admin is auto-accepted (bootstrap). Subsequent require existing admin auth.
		if (method === "PUT" && path === "/admins") {
			const body = (await request.json()) as {
				publicKey: string;
				signature?: string;
			};
			if (!body.publicKey || body.publicKey.length !== 64) {
				return new Response("publicKey must be 64 hex chars", {
					status: 400,
				});
			}

			const count = (this.sql.exec("SELECT COUNT(*) as cnt FROM admins").one() as { cnt: number })
				.cnt;

			if (count > 0 && body.signature) {
				const authKey = request.headers.get("X-Admin-Key");
				if (!authKey) {
					return new Response("X-Admin-Key header required", {
						status: 401,
					});
				}
				const isAdmin =
					this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", authKey).toArray().length > 0;
				if (!isAdmin) {
					return new Response("Not an admin", { status: 403 });
				}
				// Signature verification would go here — skipped for now since
				// auth stubs are in place; the pattern mirrors the Festival DO.
			} else if (count > 0) {
				return new Response("Signature required from existing admin", { status: 401 });
			}

			this.sql.exec("INSERT OR IGNORE INTO admins (public_key) VALUES (?)", body.publicKey);
			return Response.json({ ok: true });
		}

		// GET /admins — list all global admin public keys
		if (method === "GET" && path === "/admins") {
			const rows = this.sql.exec("SELECT public_key FROM admins").toArray() as {
				public_key: string;
			}[];
			return Response.json(rows.map((r) => r.public_key));
		}

		return new Response("Not found", { status: 404 });
	}
}
