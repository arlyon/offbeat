import { DurableObject } from "cloudflare:workers";
import { ed25519 } from "@noble/curves/ed25519.js";
import type {
	ArtistBillingResolution,
	ArtistCredit,
	ArtistProfile,
	ClashfinderApiResponse,
	ClashfinderSource,
	Lineup,
	PerformanceQualifier,
} from "@offbeat/protocol";
import {
	fetchClashfinder,
	normalizeArtistBilling,
	parseArtistBilling,
	parseClashfinderApi,
} from "@offbeat/protocol";
import {
	type ArtistEnrichmentMessage,
	type ArtistEnrichmentOutcome,
	artistEnrichmentJobId,
	artistEnrichmentSourceKey,
} from "./artist-enrichment";
import {
	ARTIST_RESOLVER_VERSION,
	artistIdentitySearchCacheKeys,
	canonicalResidentAdvisorProfileUrl,
	sha256CanonicalJson,
} from "./artist-resolution";
import {
	generateAuthenticationOptions,
	generateRegistrationOptions,
	getExpectedOrigins,
	verifyAuthentication,
	verifyRegistration,
} from "./auth";
import {
	festivalImportSigningPayload,
	IMPORT_GLOBAL_PREVIEW_LIMIT,
	IMPORT_GLOBAL_PUBLISH_LIMIT,
	IMPORT_NETWORK_PREVIEW_LIMIT,
	IMPORT_NETWORK_PUBLISH_LIMIT,
	IMPORT_PREVIEW_LIMIT,
	IMPORT_PREVIEW_TTL_SECONDS,
	IMPORT_PREVIEW_WINDOW_SECONDS,
	IMPORT_PUBLISH_LIMIT,
	IMPORT_PUBLISH_WINDOW_SECONDS,
	IMPORT_REQUEST_MAX_SKEW_SECONDS,
	MAX_CLASHFINDER_RESPONSE_BYTES,
	MAX_IMPORT_REQUEST_BYTES,
	normalizeClashfinderId,
	validateClashfinderImport,
} from "./festival-import";
import { generateKeypair, sign, verify } from "./signing";

function requireUrl(value: string): URL {
	try {
		return new URL(value);
	} catch (error) {
		throw new Error(`Invalid request URL: ${value}`, { cause: error });
	}
}

async function readBoundedRequestBody(
	headers: Headers,
	bodyStream: ReadableStream<Uint8Array> | null,
	maxBytes: number,
): Promise<Uint8Array<ArrayBuffer> | Response> {
	const contentLength = Number(headers.get("Content-Length") ?? "0");
	if (Number.isFinite(contentLength) && contentLength > maxBytes) {
		return new Response("Artist admin request is too large", { status: 413 });
	}
	if (!bodyStream) return new Uint8Array(new ArrayBuffer(0));
	const reader = bodyStream.getReader();
	const chunks: Uint8Array[] = [];
	let total = 0;
	while (true) {
		const { done, value } = await reader.read();
		if (done) break;
		total += value.byteLength;
		if (total > maxBytes) {
			await reader.cancel();
			return new Response("Artist admin request is too large", { status: 413 });
		}
		chunks.push(value);
	}
	const body = new Uint8Array(new ArrayBuffer(total));
	let offset = 0;
	for (const chunk of chunks) {
		body.set(chunk, offset);
		offset += chunk.byteLength;
	}
	return body;
}

function parseStoredJson<T>(value: string, label: string): T {
	try {
		return JSON.parse(value) as T;
	} catch (error) {
		throw new Error(`Invalid JSON in ${label}`, { cause: error });
	}
}

function stableResolutionId(billingKey: string): string {
	let hash = 2166136261;
	for (let index = 0; index < billingKey.length; index += 1) {
		hash ^= billingKey.charCodeAt(index);
		hash = Math.imul(hash, 16777619);
	}
	return `artist-resolution-v1-${(hash >>> 0).toString(16).padStart(8, "0")}`;
}

interface ArtistIdentityInput {
	name: string;
	mbid?: string;
	residentAdvisorUrl?: string;
	aliases?: string[];
	removeLinkUrls?: string[];
	relations?: Array<{ kind: "member_of"; artistId: string }>;
}

interface ArtistIdentityMergeInput {
	fromArtistId: string;
	toArtistId: string;
}

interface ArtistOverrideInput {
	billingKey: string;
	credits: Array<{
		artistId: string;
		creditedAs: string;
		role: "performer" | "presenter" | "guest";
	}>;
	presentedTitle?: string;
	performanceQualifiers?: PerformanceQualifier[];
}

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
		// Priority 1: Environment variable (useful for pinning the root key in the app)
		const envSecret = (this.env as { MAIN_DO_ROOT_SECRET?: string }).MAIN_DO_ROOT_SECRET;
		if (envSecret && /^[0-9a-f]{64}$/i.test(envSecret)) {
			const secretKey = hexToBytes(envSecret);
			this.#secretKey = secretKey;
			this.#publicKey = ed25519.getPublicKey(secretKey);
			console.log("[main] root keypair initialized from environment");
			return;
		}

		// Priority 2: Stored in DO storage
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
			CREATE TABLE IF NOT EXISTS main_schema_migrations (
				name TEXT PRIMARY KEY,
				completed_at INTEGER NOT NULL
			);

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
				id TEXT NOT NULL,
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				name TEXT NOT NULL,
				short TEXT NOT NULL,
				color TEXT NOT NULL,
				sort_order INTEGER NOT NULL,
				PRIMARY KEY (festival_id, id)
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
				source_billing TEXT NOT NULL,
				billing_key TEXT NOT NULL,
				artist_mbid TEXT,
				artist_ids TEXT NOT NULL DEFAULT '[]',
				billing_resolution_id TEXT,
				artist_credits TEXT NOT NULL DEFAULT '[]',
				presented_title TEXT,
				performance_qualifiers TEXT NOT NULL DEFAULT '[]',
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

			CREATE TABLE IF NOT EXISTS festival_import_previews (
				id TEXT PRIMARY KEY,
				public_key TEXT NOT NULL,
				clashfinder_id TEXT NOT NULL,
				festival_id TEXT NOT NULL,
				name TEXT NOT NULL,
				start_date TEXT NOT NULL,
				end_date TEXT NOT NULL,
				year INTEGER NOT NULL,
				stage_count INTEGER NOT NULL,
				set_count INTEGER NOT NULL,
				lineup_json TEXT NOT NULL,
				expires_at INTEGER NOT NULL,
				created_at INTEGER NOT NULL
			);

			CREATE INDEX IF NOT EXISTS idx_festival_import_previews_expiry
				ON festival_import_previews(expires_at);

			CREATE TABLE IF NOT EXISTS festival_import_nonces (
				nonce TEXT PRIMARY KEY,
				public_key TEXT NOT NULL,
				created_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS festival_import_audit (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				public_key TEXT NOT NULL,
				action TEXT NOT NULL,
				clashfinder_id TEXT,
				result TEXT NOT NULL,
				created_at INTEGER NOT NULL
			);

			CREATE INDEX IF NOT EXISTS idx_festival_import_audit_rate
				ON festival_import_audit(public_key, action, created_at);

			CREATE TABLE IF NOT EXISTS festival_import_network_audit (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				network_key TEXT NOT NULL,
				action TEXT NOT NULL,
				created_at INTEGER NOT NULL
			);

			CREATE INDEX IF NOT EXISTS idx_festival_import_network_rate
				ON festival_import_network_audit(network_key, action, created_at);

			CREATE TABLE IF NOT EXISTS festival_import_results (
				preview_id TEXT PRIMARY KEY,
				public_key TEXT NOT NULL,
				festival_id TEXT NOT NULL,
				expires_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS festival_artists (
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				id TEXT NOT NULL,
				source_key TEXT NOT NULL,
				profile_json TEXT NOT NULL,
				updated_at INTEGER NOT NULL,
				PRIMARY KEY (festival_id, id),
				UNIQUE (festival_id, source_key)
			);

			CREATE TABLE IF NOT EXISTS artist_enrichment_cache (
				source_key TEXT PRIMARY KEY,
				outcome_json TEXT NOT NULL,
				expires_at INTEGER NOT NULL,
				updated_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_enrichment_jobs (
				id TEXT PRIMARY KEY,
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				source_key TEXT NOT NULL,
				billing TEXT NOT NULL,
				mbid TEXT,
				set_ids_json TEXT NOT NULL,
				status TEXT NOT NULL DEFAULT 'pending',
				attempts INTEGER NOT NULL DEFAULT 0,
				last_error TEXT,
				updated_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS canonical_artists (
				id TEXT PRIMARY KEY,
				source_key TEXT NOT NULL,
				profile_json TEXT NOT NULL,
				updated_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS canonical_artist_names (
				normalized_name TEXT NOT NULL,
				artist_id TEXT NOT NULL,
				display_name TEXT NOT NULL,
				kind TEXT NOT NULL,
				updated_at INTEGER NOT NULL,
				PRIMARY KEY (normalized_name, artist_id, kind)
			);
			CREATE INDEX IF NOT EXISTS idx_canonical_artist_names_artist
				ON canonical_artist_names(artist_id);

			CREATE TABLE IF NOT EXISTS canonical_artist_identifiers (
				namespace TEXT NOT NULL,
				value TEXT NOT NULL,
				artist_id TEXT NOT NULL,
				updated_at INTEGER NOT NULL,
				PRIMARY KEY (namespace, value, artist_id)
			);
			CREATE INDEX IF NOT EXISTS idx_canonical_artist_identifiers_artist
				ON canonical_artist_identifiers(artist_id);

			CREATE TABLE IF NOT EXISTS canonical_artist_search_terms (
				term TEXT NOT NULL,
				artist_id TEXT NOT NULL,
				weight INTEGER NOT NULL,
				PRIMARY KEY (term, artist_id)
			);
			CREATE INDEX IF NOT EXISTS idx_canonical_artist_search_terms_artist
				ON canonical_artist_search_terms(artist_id);

			CREATE TABLE IF NOT EXISTS canonical_artist_relations (
				from_artist_id TEXT NOT NULL,
				kind TEXT NOT NULL,
				to_artist_id TEXT NOT NULL,
				relation_json TEXT NOT NULL,
				updated_at INTEGER NOT NULL,
				PRIMARY KEY (from_artist_id, kind, to_artist_id)
			);
			CREATE INDEX IF NOT EXISTS idx_canonical_artist_relations_target
				ON canonical_artist_relations(to_artist_id, kind);

			CREATE TABLE IF NOT EXISTS canonical_artist_redirects (
				from_artist_id TEXT PRIMARY KEY,
				to_artist_id TEXT NOT NULL,
				updated_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_billing_resolutions (
				id TEXT PRIMARY KEY,
				billing_key TEXT NOT NULL UNIQUE,
				source_billing TEXT NOT NULL,
				status TEXT NOT NULL,
				method TEXT NOT NULL,
				confidence REAL NOT NULL,
				resolution_json TEXT NOT NULL,
				input_hash TEXT NOT NULL,
				processor_version TEXT NOT NULL,
				model TEXT,
				version INTEGER NOT NULL,
				updated_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_resolution_search_cache (
				cache_key TEXT PRIMARY KEY,
				provider TEXT NOT NULL,
				response_json TEXT NOT NULL,
				retrieved_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_resolution_ai_cache (
				cache_key TEXT PRIMARY KEY,
				model TEXT NOT NULL,
				response_json TEXT NOT NULL,
				created_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_search_provider_attempts (
				identity_key TEXT PRIMARY KEY,
				attempt_mask INTEGER NOT NULL,
				updated_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_admin_nonces (
				nonce TEXT PRIMARY KEY,
				public_key TEXT NOT NULL,
				created_at INTEGER NOT NULL
			);

			CREATE TABLE IF NOT EXISTS artist_billing_overrides (
				scope TEXT NOT NULL,
				scope_id TEXT NOT NULL,
				billing_key TEXT NOT NULL,
				resolution_json TEXT NOT NULL,
				revision INTEGER NOT NULL,
				updated_at INTEGER NOT NULL,
				PRIMARY KEY (scope, scope_id, billing_key)
			);

			CREATE TABLE IF NOT EXISTS festival_resolution_applications (
				festival_id TEXT NOT NULL REFERENCES festivals(id),
				billing_key TEXT NOT NULL,
				resolution_id TEXT NOT NULL,
				resolution_version INTEGER NOT NULL,
				set_ids_json TEXT NOT NULL,
				applied_at INTEGER,
				updated_at INTEGER NOT NULL,
				PRIMARY KEY (festival_id, billing_key)
			);
		`);
		this.#migrateFestivalStagesPrimaryKey();
		this.#migrateFestivalSetColumns();
		this.#migrateResolutionApplicationColumns();
		this.#migrateCanonicalArtists();
		this.#migrateCanonicalArtistIndex();
	}

	#migrateResolutionApplicationColumns() {
		const columns = new Set(
			(
				this.sql.exec("PRAGMA table_info(festival_resolution_applications)").toArray() as Array<{
					name: string;
				}>
			).map((column) => column.name),
		);
		if (!columns.has("applied_at")) {
			this.sql.exec("ALTER TABLE festival_resolution_applications ADD COLUMN applied_at INTEGER");
		}
	}

	#migrateCanonicalArtists() {
		if (this.#migrationComplete("canonical-artists-v1")) return;
		const rows = this.sql
			.exec(
				`SELECT id, source_key, profile_json, updated_at FROM festival_artists
				 ORDER BY updated_at DESC`,
			)
			.toArray() as unknown as Array<{
			id: string;
			source_key: string;
			profile_json: string;
			updated_at: number;
		}>;
		this.ctx.storage.transactionSync(() => {
			for (const row of rows) {
				this.sql.exec(
					`INSERT OR IGNORE INTO canonical_artists
					 (id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?)`,
					row.id,
					row.source_key,
					row.profile_json,
					row.updated_at,
				);
			}
			this.#completeMigration("canonical-artists-v1");
		});
	}

	#migrateCanonicalArtistIndex() {
		if (this.#migrationComplete("canonical-artist-index-v1")) return;
		const rows = this.sql
			.exec("SELECT source_key, profile_json, updated_at FROM canonical_artists ORDER BY id")
			.toArray() as unknown as Array<{
			source_key: string;
			profile_json: string;
			updated_at: number;
		}>;
		this.ctx.storage.transactionSync(() => {
			this.sql.exec("DELETE FROM canonical_artist_names");
			this.sql.exec("DELETE FROM canonical_artist_identifiers");
			this.sql.exec("DELETE FROM canonical_artist_search_terms");
			this.sql.exec("DELETE FROM canonical_artist_relations");
			for (const row of rows) {
				this.#indexCanonicalArtist(
					parseStoredJson<ArtistProfile>(row.profile_json, "canonical artist index"),
					row.source_key,
					row.updated_at,
				);
			}
			this.#completeMigration("canonical-artist-index-v1");
		});
	}

	#artistSearchTerms(value: string): string[] {
		return [
			...new Set(
				(normalizeArtistBilling(value).match(/[\p{L}\p{N}@]+/gu) ?? []).filter(
					(term) => term.length >= 2,
				),
			),
		].slice(0, 200);
	}

	#mergeCanonicalArtistProfile(incoming: ArtistProfile): ArtistProfile {
		const rows = this.sql
			.exec("SELECT profile_json FROM canonical_artists WHERE id = ? LIMIT 1", incoming.id)
			.toArray() as unknown as Array<{ profile_json: string }>;
		if (!rows[0]) return incoming;
		const existing = parseStoredJson<ArtistProfile>(
			rows[0].profile_json,
			`canonical artist ${incoming.id}`,
		);
		return {
			...existing,
			...incoming,
			mbid: incoming.mbid ?? existing.mbid,
			aliases: [...new Set([...existing.aliases, ...incoming.aliases])],
			country: incoming.country ?? existing.country,
			artistType: incoming.artistType ?? existing.artistType,
			genres: [...new Set([...existing.genres, ...incoming.genres])],
			description: incoming.description ?? existing.description,
			links: [
				...new Map(
					[...existing.links, ...incoming.links].map((link) => [`${link.kind}:${link.url}`, link]),
				).values(),
			],
			relations: [
				...new Map(
					[...(existing.relations ?? []), ...(incoming.relations ?? [])].map((relation) => [
						`${relation.kind}:${relation.artistId}`,
						relation,
					]),
				).values(),
			],
			provenance: [
				...new Map(
					[...existing.provenance, ...incoming.provenance].map((item) => [
						`${item.field}:${item.provider}:${item.sourceUrl}:${item.retrievedAt}`,
						item,
					]),
				).values(),
			],
		};
	}

	#indexCanonicalArtist(profile: ArtistProfile, sourceKey: string, updatedAt: number) {
		this.sql.exec("DELETE FROM canonical_artist_names WHERE artist_id = ?", profile.id);
		this.sql.exec("DELETE FROM canonical_artist_identifiers WHERE artist_id = ?", profile.id);
		this.sql.exec("DELETE FROM canonical_artist_search_terms WHERE artist_id = ?", profile.id);
		this.sql.exec("DELETE FROM canonical_artist_relations WHERE from_artist_id = ?", profile.id);

		for (const [displayName, kind] of [
			[profile.name, "canonical"],
			...profile.aliases.map((alias) => [alias, "alias"]),
		] as Array<[string, string]>) {
			const normalizedName = normalizeArtistBilling(displayName);
			if (!normalizedName) continue;
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artist_names
				 (normalized_name, artist_id, display_name, kind, updated_at) VALUES (?, ?, ?, ?, ?)`,
				normalizedName,
				profile.id,
				displayName,
				kind,
				updatedAt,
			);
			for (const term of this.#artistSearchTerms(displayName)) {
				this.sql.exec(
					`INSERT INTO canonical_artist_search_terms (term, artist_id, weight)
					 VALUES (?, ?, ?) ON CONFLICT (term, artist_id)
					 DO UPDATE SET weight = MAX(weight, excluded.weight)`,
					term,
					profile.id,
					kind === "canonical" ? 10 : 8,
				);
			}
		}
		for (const value of [...profile.genres, profile.description ?? ""]) {
			for (const term of this.#artistSearchTerms(value)) {
				this.sql.exec(
					`INSERT INTO canonical_artist_search_terms (term, artist_id, weight)
					 VALUES (?, ?, ?) ON CONFLICT (term, artist_id)
					 DO UPDATE SET weight = MAX(weight, excluded.weight)`,
					term,
					profile.id,
					2,
				);
			}
		}
		const identifiers: Array<[string, string]> = [["source_key", sourceKey]];
		const separator = profile.id.indexOf(":");
		if (separator > 0)
			identifiers.push([profile.id.slice(0, separator), profile.id.slice(separator + 1)]);
		if (profile.mbid) identifiers.push(["mbid", profile.mbid.toLowerCase()]);
		for (const link of profile.links) identifiers.push([link.kind, link.url]);
		for (const [namespace, value] of new Map(
			identifiers.map((identifier) => [`${identifier[0]}:${identifier[1]}`, identifier]),
		).values()) {
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artist_identifiers
				 (namespace, value, artist_id, updated_at) VALUES (?, ?, ?, ?)`,
				namespace,
				value,
				profile.id,
				updatedAt,
			);
		}
		for (const relation of profile.relations ?? []) {
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artist_relations
				 (from_artist_id, kind, to_artist_id, relation_json, updated_at)
				 VALUES (?, ?, ?, ?, ?)`,
				profile.id,
				relation.kind,
				relation.artistId,
				JSON.stringify(relation),
				updatedAt,
			);
		}
	}

	#migrateFestivalSetColumns() {
		const columns = new Set(
			(this.sql.exec("PRAGMA table_info(festival_sets)").toArray() as Array<{ name: string }>).map(
				(column) => column.name,
			),
		);
		if (!columns.has("artist_mbid")) {
			this.sql.exec("ALTER TABLE festival_sets ADD COLUMN artist_mbid TEXT");
		}
		if (!columns.has("artist_ids")) {
			this.sql.exec("ALTER TABLE festival_sets ADD COLUMN artist_ids TEXT NOT NULL DEFAULT '[]'");
		}
		if (!columns.has("source_billing")) {
			this.sql.exec("ALTER TABLE festival_sets ADD COLUMN source_billing TEXT");
		}
		if (!columns.has("billing_key")) {
			this.sql.exec("ALTER TABLE festival_sets ADD COLUMN billing_key TEXT");
		}
		if (!columns.has("billing_resolution_id")) {
			this.sql.exec("ALTER TABLE festival_sets ADD COLUMN billing_resolution_id TEXT");
		}
		if (!columns.has("artist_credits")) {
			this.sql.exec(
				"ALTER TABLE festival_sets ADD COLUMN artist_credits TEXT NOT NULL DEFAULT '[]'",
			);
		}
		if (!columns.has("presented_title")) {
			this.sql.exec("ALTER TABLE festival_sets ADD COLUMN presented_title TEXT");
		}
		if (!columns.has("performance_qualifiers")) {
			this.sql.exec(
				"ALTER TABLE festival_sets ADD COLUMN performance_qualifiers TEXT NOT NULL DEFAULT '[]'",
			);
		}

		if (this.#migrationComplete("artist-billing-columns-v1")) {
			this.sql.exec(
				"CREATE INDEX IF NOT EXISTS idx_festival_sets_billing_key ON festival_sets(festival_id, billing_key)",
			);
			return;
		}
		const rows = this.sql
			.exec("SELECT festival_id, id, artist, artist_mbid FROM festival_sets")
			.toArray() as unknown as Array<{
			festival_id: string;
			id: string;
			artist: string;
			artist_mbid: string | null;
		}>;
		for (const row of rows) {
			const parsed = parseArtistBilling(row.artist, row.artist_mbid ?? undefined);
			this.sql.exec(
				`UPDATE festival_sets
				 SET source_billing = COALESCE(source_billing, ?),
				     billing_key = COALESCE(billing_key, ?)
				 WHERE festival_id = ? AND id = ?`,
				row.artist,
				parsed.billingKey,
				row.festival_id,
				row.id,
			);
		}
		this.sql.exec(
			"CREATE INDEX IF NOT EXISTS idx_festival_sets_billing_key ON festival_sets(festival_id, billing_key)",
		);
		this.#completeMigration("artist-billing-columns-v1");
	}

	#migrationComplete(name: string): boolean {
		return (
			this.sql.exec("SELECT 1 FROM main_schema_migrations WHERE name = ?", name).toArray().length >
			0
		);
	}

	#completeMigration(name: string) {
		this.sql.exec(
			"INSERT OR IGNORE INTO main_schema_migrations (name, completed_at) VALUES (?, ?)",
			name,
			Math.floor(Date.now() / 1000),
		);
	}

	#migrateFestivalStagesPrimaryKey() {
		const primaryKeyColumns = (
			this.sql.exec("PRAGMA table_info(festival_stages)").toArray() as Array<{
				name: string;
				pk: number;
			}>
		)
			.filter((column) => column.pk > 0)
			.sort((left, right) => left.pk - right.pk)
			.map((column) => column.name);
		if (primaryKeyColumns.join(",") === "festival_id,id") return;

		this.ctx.storage.transactionSync(() => {
			this.sql.exec(`
				CREATE TABLE festival_stages_v2 (
					id TEXT NOT NULL,
					festival_id TEXT NOT NULL REFERENCES festivals(id),
					name TEXT NOT NULL,
					short TEXT NOT NULL,
					color TEXT NOT NULL,
					sort_order INTEGER NOT NULL,
					PRIMARY KEY (festival_id, id)
				)
			`);
			this.sql.exec(`
				INSERT INTO festival_stages_v2 (id, festival_id, name, short, color, sort_order)
				SELECT id, festival_id, name, short, color, sort_order FROM festival_stages
			`);
			this.sql.exec("DROP TABLE festival_stages");
			this.sql.exec("ALTER TABLE festival_stages_v2 RENAME TO festival_stages");
		});
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
			const sourceBilling = set.sourceBilling ?? set.artist;
			const parsed = parseArtistBilling(sourceBilling, set.artistMbid);
			this.sql.exec(
				`INSERT INTO festival_sets
				 (id, festival_id, day_id, stage_id, artist, source_billing, billing_key,
				  artist_mbid, artist_ids, billing_resolution_id, artist_credits,
				  presented_title, performance_qualifiers, start_min, duration_min, genre, cancelled)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
				set.id,
				festivalId,
				set.day,
				set.stage,
				set.artist,
				sourceBilling,
				parsed.billingKey,
				set.artistMbid ?? null,
				JSON.stringify(set.artistIds ?? []),
				set.billingResolutionId ?? null,
				JSON.stringify(set.artistCredits ?? []),
				set.presentedTitle ?? parsed.presentedTitle ?? null,
				JSON.stringify(set.performanceQualifiers ?? parsed.performanceQualifiers),
				set.startMin,
				set.durationMin,
				set.genre,
				set.cancelled ? 1 : 0,
			);
		}
		this.#reapplyCachedArtistResolutions(festivalId);
	}

	#reapplyCachedArtistResolutions(festivalId: string) {
		const keys = this.sql
			.exec(
				"SELECT DISTINCT billing_key FROM festival_sets WHERE festival_id = ? AND billing_key IS NOT NULL",
				festivalId,
			)
			.toArray() as unknown as Array<{ billing_key: string }>;
		for (const { billing_key: billingKey } of keys) {
			const overrideRows = this.sql
				.exec(
					`SELECT resolution_json FROM artist_billing_overrides
					 WHERE billing_key = ? AND ((scope = 'festival' AND scope_id = ?) OR scope = 'global')
					 ORDER BY CASE scope WHEN 'festival' THEN 0 ELSE 1 END LIMIT 1`,
					billingKey,
					festivalId,
				)
				.toArray() as unknown as Array<{ resolution_json: string }>;
			const cachedRows = this.sql
				.exec(
					"SELECT resolution_json FROM artist_billing_resolutions WHERE billing_key = ? AND status = 'resolved' LIMIT 1",
					billingKey,
				)
				.toArray() as unknown as Array<{ resolution_json: string }>;
			const serialized = overrideRows[0]?.resolution_json ?? cachedRows[0]?.resolution_json;
			if (!serialized) continue;
			const resolution = parseStoredJson<ArtistBillingResolution>(
				serialized,
				`artist billing resolution ${billingKey}`,
			);
			this.#applyResolutionToFestivalSets(festivalId, resolution);
		}
	}

	#applyResolutionToFestivalSets(festivalId: string, resolution: ArtistBillingResolution) {
		const rows = this.sql
			.exec(
				"SELECT id FROM festival_sets WHERE festival_id = ? AND billing_key = ? ORDER BY id",
				festivalId,
				resolution.billingKey,
			)
			.toArray() as unknown as Array<{ id: string }>;
		if (rows.length === 0) return;
		const setIds = rows.map((row) => row.id);
		const artistIds = [...new Set(resolution.credits.map((credit) => credit.artistId))].sort(
			(left, right) => left.localeCompare(right),
		);
		this.sql.exec(
			`UPDATE festival_sets
			 SET artist_ids = ?, billing_resolution_id = ?, artist_credits = ?,
			     presented_title = ?, performance_qualifiers = ?
			 WHERE festival_id = ? AND billing_key = ?`,
			JSON.stringify(artistIds),
			resolution.id,
			JSON.stringify(resolution.credits),
			resolution.presentedTitle ?? null,
			JSON.stringify(resolution.performanceQualifiers),
			festivalId,
			resolution.billingKey,
		);
		for (const artistId of artistIds) {
			const profiles = this.sql
				.exec(
					"SELECT source_key, profile_json, updated_at FROM canonical_artists WHERE id = ? LIMIT 1",
					artistId,
				)
				.toArray() as unknown as Array<{
				source_key: string;
				profile_json: string;
				updated_at: number;
			}>;
			if (!profiles[0]) continue;
			this.sql.exec(
				`INSERT OR REPLACE INTO festival_artists
				 (festival_id, id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?, ?)`,
				festivalId,
				artistId,
				profiles[0].source_key,
				profiles[0].profile_json,
				profiles[0].updated_at,
			);
		}
		this.sql.exec(
			`INSERT OR REPLACE INTO festival_resolution_applications
			 (festival_id, billing_key, resolution_id, resolution_version, set_ids_json, applied_at,
			  updated_at)
			 VALUES (?, ?, ?, ?, ?, NULL, ?)`,
			festivalId,
			resolution.billingKey,
			resolution.id,
			resolution.version,
			JSON.stringify(setIds),
			Math.floor(Date.now() / 1000),
		);
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
				genres: parseStoredJson<unknown[]>(row.genres as string, `festival ${row.id} genres`),
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
			genres: parseStoredJson<unknown[]>(row.genres as string, `festival ${row.id} genres`),
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

		const url = requireUrl(request.url);
		const message = new TextEncoder().encode(url.pathname);
		const valid = await verify(hexToBytes(pubKeyHex), message, hexToBytes(sigHex));
		if (!valid) {
			return new Response("Invalid signature", { status: 401 });
		}

		return null;
	}

	async #requireArtistResolutionAdmin(request: Request): Promise<Response | null> {
		const pubKeyHex = request.headers.get("X-Admin-Key")?.toLowerCase();
		const sigHex = request.headers.get("X-Admin-Sig");
		const timestampText = request.headers.get("X-Admin-Timestamp");
		const nonce = request.headers.get("X-Admin-Nonce")?.toLowerCase();
		if (!pubKeyHex || !sigHex || !timestampText || !nonce) {
			return new Response("Artist admin authentication headers required", { status: 401 });
		}
		if (!/^[0-9a-f]{64}$/.test(pubKeyHex) || !/^[0-9a-f]{128}$/i.test(sigHex)) {
			return new Response("Invalid artist admin credentials", { status: 401 });
		}
		if (!/^[a-z0-9_-]{16,128}$/.test(nonce)) {
			return new Response("Invalid artist admin nonce", { status: 401 });
		}
		const timestamp = Number(timestampText);
		const now = Math.floor(Date.now() / 1000);
		if (!Number.isSafeInteger(timestamp) || Math.abs(now - timestamp) > 300) {
			return new Response("Artist admin timestamp is stale", { status: 401 });
		}
		const isAdmin =
			this.sql.exec("SELECT 1 FROM admins WHERE public_key = ?", pubKeyHex).toArray().length > 0;
		if (!isAdmin) return new Response("Not an admin", { status: 403 });
		const requestClone = request.clone();
		const body = await readBoundedRequestBody(
			requestClone.headers,
			requestClone.body,
			MAX_IMPORT_REQUEST_BYTES,
		);
		if (body instanceof Response) return body;
		const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", body));
		const bodyHash = bytesToHex(digest);
		const url = requireUrl(request.url);
		const message = new TextEncoder().encode(
			`${request.method.toUpperCase()}\n${url.pathname}\n${timestampText}\n${nonce}\n${bodyHash}`,
		);
		const valid = await verify(hexToBytes(pubKeyHex), message, hexToBytes(sigHex));
		if (!valid) return new Response("Invalid signature", { status: 401 });
		this.sql.exec("DELETE FROM artist_admin_nonces WHERE created_at < ?", now - 600);
		try {
			this.sql.exec(
				"INSERT INTO artist_admin_nonces (nonce, public_key, created_at) VALUES (?, ?, ?)",
				nonce,
				pubKeyHex,
				now,
			);
		} catch {
			return new Response("Artist admin request was already used", { status: 409 });
		}
		return null;
	}

	async #requireRegisteredImportUser(
		request: Request,
		body: string,
	): Promise<{ publicKey: string; nonce: string } | Response> {
		if (!this.#publicKey) return new Response("MainDO keypair not initialized", { status: 503 });
		const attestationMessage = request.headers.get("X-Attestation-Message");
		const attestationSignature = request.headers.get("X-Attestation-Signature");
		const attestationIssuer = request.headers.get("X-Attestation-Issuer");
		const publicKey = request.headers.get("X-Session-PublicKey")?.toLowerCase();
		const requestTimestamp = request.headers.get("X-Request-Timestamp");
		const requestNonce = request.headers.get("X-Request-Nonce")?.toLowerCase();
		const requestSignature = request.headers.get("X-Request-Signature");
		if (
			!attestationMessage ||
			!attestationSignature ||
			!attestationIssuer ||
			!publicKey ||
			!requestTimestamp ||
			!requestNonce ||
			!requestSignature
		) {
			return new Response("Registered-user authentication headers required", { status: 401 });
		}
		if (
			!/^([0-9a-f]{64})$/.test(publicKey) ||
			!/^([0-9a-f]{128})$/i.test(attestationSignature) ||
			!/^([0-9a-f]{128})$/i.test(requestSignature) ||
			!/^([0-9a-f]{32})$/.test(requestNonce)
		) {
			return new Response("Malformed registered-user authentication", { status: 401 });
		}

		const rootPublicKey = bytesToHex(this.#publicKey);
		if (attestationIssuer.toLowerCase() !== rootPublicKey) {
			return new Response("Unknown attestation issuer", { status: 401 });
		}
		const attestationValid = await verify(
			this.#publicKey,
			new TextEncoder().encode(attestationMessage),
			hexToBytes(attestationSignature),
		);
		if (!attestationValid) return new Response("Invalid attestation", { status: 401 });

		const parts = attestationMessage.split(":");
		const issuedAt = Number(parts[3]);
		const expiresAt = Number(parts[4]);
		const now = Math.floor(Date.now() / 1000);
		if (
			parts.length !== 5 ||
			parts[0] !== "attestation" ||
			parts[1] !== "v1" ||
			parts[2]?.toLowerCase() !== publicKey ||
			!Number.isSafeInteger(issuedAt) ||
			!Number.isSafeInteger(expiresAt) ||
			issuedAt > now + IMPORT_REQUEST_MAX_SKEW_SECONDS ||
			expiresAt < now
		) {
			return new Response("Expired or invalid attestation", { status: 401 });
		}
		const registered = this.sql
			.exec("SELECT 1 FROM credentials WHERE public_key = ? LIMIT 1", publicKey)
			.toArray();
		const revoked = this.sql
			.exec("SELECT 1 FROM revocations WHERE public_key = ? LIMIT 1", publicKey)
			.toArray();
		if (registered.length === 0 || revoked.length > 0) {
			return new Response("User is not registered", { status: 403 });
		}

		const timestamp = Number(requestTimestamp);
		if (
			!Number.isSafeInteger(timestamp) ||
			Math.abs(now - timestamp) > IMPORT_REQUEST_MAX_SKEW_SECONDS
		) {
			return new Response("Stale import request", { status: 401 });
		}
		const url = requireUrl(request.url);
		const signaturePayload = festivalImportSigningPayload(
			request.method,
			url.pathname,
			requestTimestamp,
			requestNonce,
			body,
		);
		const signatureValid = await verify(
			hexToBytes(publicKey),
			new TextEncoder().encode(signaturePayload),
			hexToBytes(requestSignature),
		);
		if (!signatureValid) return new Response("Invalid import request signature", { status: 401 });

		const replayed = this.sql
			.exec("SELECT 1 FROM festival_import_nonces WHERE nonce = ? LIMIT 1", requestNonce)
			.toArray();
		if (replayed.length > 0)
			return new Response("Import request was already used", { status: 409 });
		return { publicKey, nonce: requestNonce };
	}

	async #beginImportAttempt(
		request: Request,
		publicKey: string,
		nonce: string,
		action: "preview" | "publish" | "retry",
		clashfinderId: string | null,
	): Promise<number | Response> {
		const now = Math.floor(Date.now() / 1000);
		let windowSeconds = IMPORT_PREVIEW_WINDOW_SECONDS;
		let userLimit = 20;
		let networkLimit = 100;
		let globalLimit = 500;
		if (action === "preview") {
			userLimit = IMPORT_PREVIEW_LIMIT;
			networkLimit = IMPORT_NETWORK_PREVIEW_LIMIT;
			globalLimit = IMPORT_GLOBAL_PREVIEW_LIMIT;
		} else if (action === "publish") {
			windowSeconds = IMPORT_PUBLISH_WINDOW_SECONDS;
			userLimit = IMPORT_PUBLISH_LIMIT;
			networkLimit = IMPORT_NETWORK_PUBLISH_LIMIT;
			globalLimit = IMPORT_GLOBAL_PUBLISH_LIMIT;
		}
		const address = request.headers.get("CF-Connecting-IP") ?? "unknown";
		const addressDigest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(address));
		const networkKey = bytesToHex(new Uint8Array(addressDigest)).slice(0, 32);
		const userCount = (
			this.sql
				.exec(
					"SELECT COUNT(*) AS count FROM festival_import_audit WHERE public_key = ? AND action = ? AND created_at >= ?",
					publicKey,
					action,
					now - windowSeconds,
				)
				.one() as { count: number }
		).count;
		const globalCount = (
			this.sql
				.exec(
					"SELECT COUNT(*) AS count FROM festival_import_audit WHERE action = ? AND created_at >= ?",
					action,
					now - windowSeconds,
				)
				.one() as { count: number }
		).count;
		const networkCount = (
			this.sql
				.exec(
					"SELECT COUNT(*) AS count FROM festival_import_network_audit WHERE network_key = ? AND action = ? AND created_at >= ?",
					networkKey,
					action,
					now - windowSeconds,
				)
				.one() as { count: number }
		).count;
		if (userCount >= userLimit || networkCount >= networkLimit || globalCount >= globalLimit) {
			return new Response("Festival import rate limit exceeded", {
				status: 429,
				headers: { "Retry-After": windowSeconds.toString() },
			});
		}
		const replayed = this.sql
			.exec("SELECT 1 FROM festival_import_nonces WHERE nonce = ? LIMIT 1", nonce)
			.toArray();
		if (replayed.length > 0)
			return new Response("Import request was already used", { status: 409 });

		let auditId = 0;
		this.ctx.storage.transactionSync(() => {
			this.sql.exec(
				"INSERT INTO festival_import_nonces (nonce, public_key, created_at) VALUES (?, ?, ?)",
				nonce,
				publicKey,
				now,
			);
			auditId = (
				this.sql
					.exec(
						"INSERT INTO festival_import_audit (public_key, action, clashfinder_id, result, created_at) VALUES (?, ?, ?, 'started', ?) RETURNING id",
						publicKey,
						action,
						clashfinderId,
						now,
					)
					.one() as { id: number }
			).id;
			this.sql.exec(
				"INSERT INTO festival_import_network_audit (network_key, action, created_at) VALUES (?, ?, ?)",
				networkKey,
				action,
				now,
			);
		});
		return auditId;
	}

	#finishImportAttempt(id: number, result: string) {
		this.sql.exec("UPDATE festival_import_audit SET result = ? WHERE id = ?", result, id);
	}

	#cleanupFestivalImports(now = Math.floor(Date.now() / 1000)) {
		this.sql.exec("DELETE FROM festival_import_previews WHERE expires_at < ?", now);
		this.sql.exec("DELETE FROM festival_import_results WHERE expires_at < ?", now);
		this.sql.exec(
			"DELETE FROM festival_import_nonces WHERE created_at < ?",
			now - IMPORT_PUBLISH_WINDOW_SECONDS,
		);
		const auditRetention = now - 30 * 24 * 60 * 60;
		this.sql.exec("DELETE FROM festival_import_audit WHERE created_at < ?", auditRetention);
		this.sql.exec("DELETE FROM festival_import_network_audit WHERE created_at < ?", auditRetention);
	}

	#getFestivalByClashfinderId(clashfinderId: string) {
		const rows = this.sql
			.exec(
				"SELECT id FROM festivals WHERE lower(clashfinder_id) = lower(?) LIMIT 1",
				clashfinderId,
			)
			.toArray() as Array<{ id: string }>;
		return rows[0] ? this.#getFestival(rows[0].id) : null;
	}

	async #fetchClashfinder(clashfinderId: string): Promise<ClashfinderApiResponse> {
		const env = this.env as Record<string, string | undefined>;
		if (
			env.RP_ID === "localhost" &&
			env.DEV_BYPASS_WEBAUTHN === "true" &&
			env.CLASHFINDER_TEST_FIXTURE
		) {
			return parseStoredJson<ClashfinderApiResponse>(
				env.CLASHFINDER_TEST_FIXTURE,
				"CLASHFINDER_TEST_FIXTURE",
			);
		}
		if (!env.CLASHFINDER_USERNAME || !env.CLASHFINDER_PRIVATE_KEY) {
			throw new Error("Clashfinder credentials not configured");
		}
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), 10_000);
		try {
			return await fetchClashfinder(
				clashfinderId,
				{ username: env.CLASHFINDER_USERNAME, privateKey: env.CLASHFINDER_PRIVATE_KEY },
				{ signal: controller.signal, maxResponseBytes: MAX_CLASHFINDER_RESPONSE_BYTES },
			);
		} finally {
			clearTimeout(timeout);
		}
	}

	/** Look up a stored WebAuthn credential by its credential ID. */
	#getCredentialById(credentialId: string): {
		id: string;
		publicKey: Uint8Array;
		ed25519PublicKey: string;
		counter: number;
		transports?: string[];
	} | null {
		const rows = this.sql
			.exec("SELECT credential_data, public_key FROM credentials WHERE id = ?", credentialId)
			.toArray() as Array<{ credential_data: string; public_key: string }>;
		if (rows.length === 0) return null;
		const data = parseStoredJson<{
			credentialId: string;
			publicKey: Record<string, number>;
			counter: number;
			transports?: string[];
		}>(rows[0].credential_data, `credential ${credentialId}`);
		// publicKey was stored as a serialized Uint8Array (JSON object with numeric keys)
		const pkBytes = new Uint8Array(Object.values(data.publicKey));
		return {
			id: data.credentialId,
			publicKey: pkBytes,
			ed25519PublicKey: rows[0].public_key,
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
			source_billing: string | null;
			artist_mbid: string | null;
			artist_ids: string;
			billing_resolution_id: string | null;
			artist_credits: string;
			presented_title: string | null;
			performance_qualifiers: string;
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
		const artists = this.sql
			.exec("SELECT profile_json FROM festival_artists WHERE festival_id = ? ORDER BY id", id)
			.toArray() as unknown as Array<{ profile_json: string }>;
		const resolutions = this.sql
			.exec(
				`SELECT DISTINCT resolution_json FROM artist_billing_resolutions
				 WHERE id IN (
				   SELECT billing_resolution_id FROM festival_sets
				   WHERE festival_id = ? AND billing_resolution_id IS NOT NULL
				 ) ORDER BY id`,
				id,
			)
			.toArray() as unknown as Array<{ resolution_json: string }>;

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
				sourceBilling: s.source_billing ?? s.artist,
				...(s.artist_mbid ? { artistMbid: s.artist_mbid } : {}),
				...(parseStoredJson<string[]>(s.artist_ids, `set ${s.id} artist IDs`).length > 0
					? { artistIds: parseStoredJson<string[]>(s.artist_ids, `set ${s.id} artist IDs`) }
					: {}),
				...(s.billing_resolution_id ? { billingResolutionId: s.billing_resolution_id } : {}),
				...(parseStoredJson<ArtistCredit[]>(s.artist_credits, `set ${s.id} artist credits`).length >
				0
					? {
							artistCredits: parseStoredJson<ArtistCredit[]>(
								s.artist_credits,
								`set ${s.id} artist credits`,
							),
						}
					: {}),
				...(s.presented_title ? { presentedTitle: s.presented_title } : {}),
				...(parseStoredJson<PerformanceQualifier[]>(
					s.performance_qualifiers,
					`set ${s.id} performance qualifiers`,
				).length > 0
					? {
							performanceQualifiers: parseStoredJson<PerformanceQualifier[]>(
								s.performance_qualifiers,
								`set ${s.id} performance qualifiers`,
							),
						}
					: {}),
				startMin: s.start_min,
				durationMin: s.duration_min,
				genre: s.genre,
				cancelled: s.cancelled === 1,
			})),
			...(artists.length > 0
				? {
						artists: artists.map((artist) =>
							parseStoredJson<ArtistProfile>(artist.profile_json, "festival artist profile"),
						),
					}
				: {}),
			...(resolutions.length > 0
				? {
						billingResolutions: resolutions.map((resolution) =>
							parseStoredJson<ArtistBillingResolution>(
								resolution.resolution_json,
								"festival artist billing resolution",
							),
						),
					}
				: {}),
		};
	}

	getArtistEnrichmentCandidates(
		festivalId: string,
		billingKeys?: string[],
	): ArtistEnrichmentMessage[] {
		if (!this.#getFestival(festivalId)) return [];
		const selectedBillingKeys = billingKeys?.length ? new Set(billingKeys.slice(0, 20)) : undefined;
		const rows = this.sql
			.exec(
				`SELECT s.id, s.artist, s.source_billing, s.billing_key, s.artist_mbid,
				        s.artist_ids, s.billing_resolution_id, a.applied_at
				 FROM festival_sets s
				 LEFT JOIN festival_resolution_applications a
				   ON a.festival_id = s.festival_id AND a.billing_key = s.billing_key
				 LEFT JOIN artist_billing_resolutions r ON r.id = s.billing_resolution_id
				 WHERE s.festival_id = ?
				   AND (
				     s.billing_resolution_id IS NULL OR a.applied_at IS NULL OR
				     (r.method <> 'manual' AND r.processor_version <> ?)
				   )
				 ORDER BY s.artist, s.id`,
				festivalId,
				ARTIST_RESOLVER_VERSION,
			)
			.toArray() as unknown as Array<{
			id: string;
			artist: string;
			source_billing: string | null;
			billing_key: string | null;
			artist_mbid: string | null;
			artist_ids: string;
			billing_resolution_id: string | null;
			applied_at: number | null;
		}>;
		const contextBillings: string[] = [];
		let contextCharacters = 0;
		for (const billing of this.getArtistBillingContext(festivalId)) {
			if (contextBillings.length >= 250 || contextCharacters + billing.length > 32_000) break;
			contextBillings.push(billing);
			contextCharacters += billing.length;
		}
		const grouped = new Map<
			string,
			{ sourceKey: string; billing: string; billingKey: string; mbid?: string; setIds: string[] }
		>();
		for (const row of rows) {
			const billing = row.source_billing ?? row.artist;
			const billingKey =
				row.billing_key ?? parseArtistBilling(billing, row.artist_mbid ?? undefined).billingKey;
			if (selectedBillingKeys && !selectedBillingKeys.has(billingKey)) continue;
			const sourceKey = artistEnrichmentSourceKey(billing, row.artist_mbid ?? undefined);
			const groupKey = `${sourceKey}|${billingKey}`;
			const existing = grouped.get(groupKey);
			if (existing) {
				existing.setIds.push(row.id);
			} else {
				grouped.set(groupKey, {
					sourceKey,
					billing,
					billingKey,
					...(row.artist_mbid ? { mbid: row.artist_mbid } : {}),
					setIds: [row.id],
				});
			}
		}

		const now = Math.floor(Date.now() / 1000);
		return [...grouped.values()].map((candidate) => {
			const setIds = [...candidate.setIds].sort((left, right) => left.localeCompare(right));
			const jobId = artistEnrichmentJobId(festivalId, candidate.billingKey, setIds);
			this.sql.exec(
				`INSERT OR IGNORE INTO artist_enrichment_jobs
				 (id, festival_id, source_key, billing, mbid, set_ids_json, status, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, 'pending', ?)`,
				jobId,
				festivalId,
				candidate.sourceKey,
				candidate.billing,
				candidate.mbid ?? null,
				JSON.stringify(setIds),
				now,
			);
			return {
				jobId,
				sourceKey: candidate.sourceKey,
				festivalId,
				setIds,
				billing: candidate.billing,
				billingKey: candidate.billingKey,
				contextBillings,
				...(candidate.mbid ? { mbid: candidate.mbid } : {}),
			};
		});
	}

	getCachedArtistEnrichment(sourceKey: string): ArtistEnrichmentOutcome | null {
		const rows = this.sql
			.exec(
				"SELECT outcome_json FROM artist_enrichment_cache WHERE source_key = ? LIMIT 1",
				sourceKey,
			)
			.toArray() as unknown as Array<{ outcome_json: string }>;
		return rows[0]
			? parseStoredJson<ArtistEnrichmentOutcome>(rows[0].outcome_json, "artist enrichment cache")
			: null;
	}

	markArtistEnrichmentQueued(jobIds: string[]) {
		const now = Math.floor(Date.now() / 1000);
		for (const jobId of jobIds) {
			this.sql.exec(
				"UPDATE artist_enrichment_jobs SET status = 'queued', updated_at = ? WHERE id = ? AND status != 'complete'",
				now,
				jobId,
			);
		}
	}

	cacheCanonicalArtistEnrichment(sourceKey: string, outcome: ArtistEnrichmentOutcome) {
		const now = Math.floor(Date.now() / 1000);
		const effectiveOutcome: ArtistEnrichmentOutcome =
			outcome.status === "enriched"
				? { status: "enriched", profile: this.#mergeCanonicalArtistProfile(outcome.profile) }
				: outcome;
		this.ctx.storage.transactionSync(() => {
			this.sql.exec(
				`INSERT OR REPLACE INTO artist_enrichment_cache
				 (source_key, outcome_json, expires_at, updated_at) VALUES (?, ?, ?, ?)`,
				sourceKey,
				JSON.stringify(effectiveOutcome),
				253_402_300_799,
				now,
			);
			if (effectiveOutcome.status === "enriched") {
				this.sql.exec(
					`INSERT OR REPLACE INTO canonical_artists
					 (id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?)`,
					effectiveOutcome.profile.id,
					sourceKey,
					JSON.stringify(effectiveOutcome.profile),
					now,
				);
				this.#indexCanonicalArtist(effectiveOutcome.profile, sourceKey, now);
			}
		});
	}

	applyArtistEnrichment(message: ArtistEnrichmentMessage, outcome: ArtistEnrichmentOutcome) {
		const now = Math.floor(Date.now() / 1000);
		const expiresAt = 253_402_300_799;
		const effectiveOutcome: ArtistEnrichmentOutcome =
			outcome.status === "enriched"
				? { status: "enriched", profile: this.#mergeCanonicalArtistProfile(outcome.profile) }
				: outcome;
		this.ctx.storage.transactionSync(() => {
			this.sql.exec(
				`INSERT OR REPLACE INTO artist_enrichment_cache
				 (source_key, outcome_json, expires_at, updated_at) VALUES (?, ?, ?, ?)`,
				message.sourceKey,
				JSON.stringify(effectiveOutcome),
				expiresAt,
				now,
			);
			this.sql.exec(
				"UPDATE artist_enrichment_jobs SET status = ?, attempts = attempts + 1, last_error = NULL, updated_at = ? WHERE id = ?",
				effectiveOutcome.status,
				now,
				message.jobId,
			);
			if (effectiveOutcome.status !== "enriched") return;
			const profile = effectiveOutcome.profile;
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artists
				 (id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?)`,
				profile.id,
				message.sourceKey,
				JSON.stringify(profile),
				now,
			);
			this.#indexCanonicalArtist(profile, message.sourceKey, now);
			this.sql.exec(
				`INSERT OR REPLACE INTO festival_artists
				 (festival_id, id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?, ?)`,
				message.festivalId,
				profile.id,
				message.sourceKey,
				JSON.stringify(profile),
				now,
			);
			for (const setId of message.setIds) {
				const rows = this.sql
					.exec(
						"SELECT artist_ids FROM festival_sets WHERE festival_id = ? AND id = ? LIMIT 1",
						message.festivalId,
						setId,
					)
					.toArray() as unknown as Array<{ artist_ids: string }>;
				if (!rows[0]) continue;
				const artistIds = new Set(
					parseStoredJson<string[]>(rows[0].artist_ids, `set ${setId} artist IDs`),
				);
				artistIds.add(profile.id);
				this.sql.exec(
					"UPDATE festival_sets SET artist_ids = ? WHERE festival_id = ? AND id = ?",
					JSON.stringify([...artistIds].sort((left, right) => left.localeCompare(right))),
					message.festivalId,
					setId,
				);
			}
		});
		return effectiveOutcome.status === "enriched"
			? { profile: effectiveOutcome.profile, setIds: message.setIds }
			: null;
	}

	getArtistBillingContext(festivalId: string): string[] {
		return (
			this.sql
				.exec(
					"SELECT DISTINCT source_billing FROM festival_sets WHERE festival_id = ? ORDER BY source_billing",
					festivalId,
				)
				.toArray() as unknown as Array<{ source_billing: string }>
		).map((row) => row.source_billing);
	}

	getCanonicalArtistProfiles(artistIds: string[]): ArtistProfile[] {
		const profiles: ArtistProfile[] = [];
		for (const requestedId of [...new Set(artistIds)].slice(0, 20)) {
			let artistId = requestedId;
			const visited = new Set<string>();
			while (!visited.has(artistId) && visited.size < 5) {
				visited.add(artistId);
				const redirects = this.sql
					.exec(
						"SELECT to_artist_id FROM canonical_artist_redirects WHERE from_artist_id = ? LIMIT 1",
						artistId,
					)
					.toArray() as unknown as Array<{ to_artist_id: string }>;
				if (!redirects[0]) break;
				artistId = redirects[0].to_artist_id;
			}
			const rows = this.sql
				.exec("SELECT profile_json FROM canonical_artists WHERE id = ? LIMIT 1", artistId)
				.toArray() as unknown as Array<{ profile_json: string }>;
			if (rows[0]) {
				const profile = parseStoredJson<ArtistProfile>(
					rows[0].profile_json,
					`canonical artist ${artistId}`,
				);
				profiles.push(requestedId === artistId ? profile : { ...profile, id: requestedId });
			}
		}
		return profiles;
	}

	getCanonicalArtistProfilesByName(names: string[]): ArtistProfile[] {
		const artistIds: string[] = [];
		for (const normalizedName of new Set(
			names.map((name) => normalizeArtistBilling(name)).filter(Boolean),
		)) {
			const rows = this.sql
				.exec(
					`SELECT artist_id FROM canonical_artist_names
					 WHERE normalized_name = ? ORDER BY kind, artist_id`,
					normalizedName,
				)
				.toArray() as unknown as Array<{ artist_id: string }>;
			artistIds.push(...rows.map((row) => row.artist_id));
		}
		return this.getCanonicalArtistProfiles(artistIds);
	}

	searchCanonicalArtistProfiles(query: string, limit = 10): ArtistProfile[] {
		const safeLimit = Math.max(1, Math.min(20, Math.floor(limit)));
		const artistIds: string[] = [];
		const exactIdentifier = this.sql
			.exec(
				`SELECT artist_id FROM canonical_artist_identifiers
				 WHERE value = ? ORDER BY artist_id LIMIT ?`,
				query.trim(),
				safeLimit,
			)
			.toArray() as unknown as Array<{ artist_id: string }>;
		artistIds.push(...exactIdentifier.map((row) => row.artist_id));
		for (const profile of this.getCanonicalArtistProfilesByName([query])) {
			artistIds.push(profile.id);
		}
		const terms = this.#artistSearchTerms(query).slice(0, 10);
		if (terms.length > 0 && artistIds.length < safeLimit) {
			const scores = new Map<string, { matchedTerms: number; score: number }>();
			for (const term of terms) {
				const rows = this.sql
					.exec("SELECT artist_id, weight FROM canonical_artist_search_terms WHERE term = ?", term)
					.toArray() as unknown as Array<{ artist_id: string; weight: number }>;
				for (const row of rows) {
					const score = scores.get(row.artist_id) ?? { matchedTerms: 0, score: 0 };
					score.matchedTerms += 1;
					score.score += row.weight;
					scores.set(row.artist_id, score);
				}
			}
			artistIds.push(
				...[...scores]
					.sort(
						([leftId, left], [rightId, right]) =>
							right.matchedTerms - left.matchedTerms ||
							right.score - left.score ||
							leftId.localeCompare(rightId),
					)
					.slice(0, safeLimit)
					.map(([artistId]) => artistId),
			);
		}
		return this.getCanonicalArtistProfiles([...new Set(artistIds)].slice(0, safeLimit));
	}

	async putAdminArtistIdentity(input: ArtistIdentityInput): Promise<ArtistProfile> {
		const name = input.name?.normalize("NFKC").trim();
		if (!name || name.length > 200) throw new Error("invalid artist name");
		const mbid = input.mbid?.trim().toLowerCase();
		if (
			mbid &&
			!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(mbid)
		) {
			throw new Error("invalid MusicBrainz artist ID");
		}
		const residentAdvisorUrl = input.residentAdvisorUrl
			? canonicalResidentAdvisorProfileUrl(input.residentAdvisorUrl)
			: undefined;
		if (input.residentAdvisorUrl && !residentAdvisorUrl) {
			throw new Error("invalid Resident Advisor artist URL");
		}
		const manualHash = await sha256CanonicalJson({ name: normalizeArtistBilling(name) });
		const artistId = mbid
			? `mbid:${mbid}`
			: residentAdvisorUrl
				? `ra:${residentAdvisorUrl.slice("https://ra.co/dj/".length).toLowerCase()}`
				: `admin:${manualHash.slice(0, 24)}`;
		const existing = this.getCanonicalArtistProfiles([artistId])[0];
		const aliases = [
			...new Set(
				[...(existing?.aliases ?? []), ...(input.aliases ?? [])]
					.map((alias) => alias.normalize("NFKC").trim())
					.filter((alias) => alias && alias.length <= 200 && alias !== name),
			),
		].slice(0, 30);
		const relations = [
			...new Map(
				[...(existing?.relations ?? []), ...(input.relations ?? [])].map((relation) => [
					`${relation.kind}:${relation.artistId}`,
					relation,
				]),
			).values(),
		].slice(0, 20);
		for (const relation of relations) {
			if (relation.kind !== "member_of" || relation.artistId === artistId) {
				throw new Error("invalid artist relation");
			}
			if (this.getCanonicalArtistProfiles([relation.artistId]).length !== 1) {
				throw new Error("related artist does not exist");
			}
		}
		const removedLinkUrls = new Set(
			(input.removeLinkUrls ?? []).map((url) => url.trim()).filter(Boolean),
		);
		if (removedLinkUrls.size > 20) throw new Error("too many artist links to remove");
		const links = (existing?.links ?? []).filter((link) => !removedLinkUrls.has(link.url));
		if (residentAdvisorUrl && !links.some((link) => link.url === residentAdvisorUrl)) {
			links.push({ kind: "resident_advisor", url: residentAdvisorUrl });
		}
		const now = new Date().toISOString();
		const sourceUrl =
			residentAdvisorUrl ?? (mbid ? `https://musicbrainz.org/artist/${mbid}` : "offbeat:admin");
		const profile: ArtistProfile = {
			...(existing ?? {
				id: artistId,
				name,
				aliases: [],
				genres: [],
				links: [],
				provenance: [],
				updatedAt: now,
			}),
			id: artistId,
			name,
			...(mbid ? { mbid } : {}),
			aliases,
			links,
			relations,
			provenance: [
				...(existing?.provenance ?? []),
				{
					field: "identity,aliases,links,relations",
					provider: "admin",
					sourceUrl,
					license: "operator-verified",
					retrievedAt: now,
				},
			],
			updatedAt: now,
		};
		const updatedAt = Math.floor(Date.now() / 1000);
		this.ctx.storage.transactionSync(() => {
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artists (id, source_key, profile_json, updated_at)
				 VALUES (?, ?, ?, ?)`,
				profile.id,
				profile.id,
				JSON.stringify(profile),
				updatedAt,
			);
			this.#indexCanonicalArtist(profile, profile.id, updatedAt);
		});
		return profile;
	}

	mergeAdminArtistIdentity(input: ArtistIdentityMergeInput): ArtistProfile {
		const fromArtistId = input.fromArtistId?.trim();
		const toArtistId = input.toArtistId?.trim();
		if (
			!fromArtistId ||
			!toArtistId ||
			fromArtistId === toArtistId ||
			fromArtistId.length > 200 ||
			toArtistId.length > 200
		) {
			throw new Error("invalid artist merge");
		}
		const loadDirect = (artistId: string) => {
			const rows = this.sql
				.exec(
					"SELECT source_key, profile_json FROM canonical_artists WHERE id = ? LIMIT 1",
					artistId,
				)
				.toArray() as unknown as Array<{ source_key: string; profile_json: string }>;
			return rows[0]
				? {
						sourceKey: rows[0].source_key,
						profile: parseStoredJson<ArtistProfile>(
							rows[0].profile_json,
							`canonical artist ${artistId}`,
						),
					}
				: null;
		};
		const source = loadDirect(fromArtistId);
		const target = loadDirect(toArtistId);
		if (!source || !target) throw new Error("artist merge identity does not exist");
		const now = new Date().toISOString();
		const updatedAt = Math.floor(Date.now() / 1000);
		const merged: ArtistProfile = {
			...target.profile,
			mbid: target.profile.mbid ?? source.profile.mbid,
			aliases: [
				...new Set([...target.profile.aliases, source.profile.name, ...source.profile.aliases]),
			].filter((alias) => alias !== target.profile.name),
			country: target.profile.country ?? source.profile.country,
			artistType: target.profile.artistType ?? source.profile.artistType,
			genres: [...new Set([...target.profile.genres, ...source.profile.genres])],
			description: target.profile.description ?? source.profile.description,
			links: [
				...new Map(
					[...target.profile.links, ...source.profile.links].map((link) => [
						`${link.kind}:${link.url}`,
						link,
					]),
				).values(),
			],
			relations: [
				...new Map(
					[...(target.profile.relations ?? []), ...(source.profile.relations ?? [])]
						.filter((relation) => relation.artistId !== toArtistId)
						.map((relation) => [`${relation.kind}:${relation.artistId}`, relation]),
				).values(),
			],
			provenance: [
				...target.profile.provenance,
				...source.profile.provenance,
				{
					field: "identity",
					provider: "admin",
					sourceUrl: "offbeat:admin",
					license: "operator-verified-merge",
					retrievedAt: now,
				},
			],
			updatedAt: now,
		};
		this.ctx.storage.transactionSync(() => {
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artists
				 (id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?)`,
				merged.id,
				target.sourceKey,
				JSON.stringify(merged),
				updatedAt,
			);
			this.#indexCanonicalArtist(merged, target.sourceKey, updatedAt);
			this.sql.exec("DELETE FROM canonical_artists WHERE id = ?", fromArtistId);
			this.sql.exec("DELETE FROM canonical_artist_names WHERE artist_id = ?", fromArtistId);
			this.sql.exec("DELETE FROM canonical_artist_identifiers WHERE artist_id = ?", fromArtistId);
			this.sql.exec("DELETE FROM canonical_artist_search_terms WHERE artist_id = ?", fromArtistId);
			this.sql.exec(
				"DELETE FROM canonical_artist_relations WHERE from_artist_id = ?",
				fromArtistId,
			);
			this.sql.exec(
				"UPDATE canonical_artist_redirects SET to_artist_id = ?, updated_at = ? WHERE to_artist_id = ?",
				toArtistId,
				updatedAt,
				fromArtistId,
			);
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artist_redirects
				 (from_artist_id, to_artist_id, updated_at) VALUES (?, ?, ?)`,
				fromArtistId,
				toArtistId,
				updatedAt,
			);
			this.sql.exec(
				`INSERT OR REPLACE INTO canonical_artist_identifiers
				 (namespace, value, artist_id, updated_at) VALUES ('artist_id', ?, ?, ?)`,
				fromArtistId,
				toArtistId,
				updatedAt,
			);
		});
		return merged;
	}

	async resetArtistSearchProviderAttemptsForBillings(
		festivalId: string,
		billingKeys: string[],
	): Promise<void> {
		for (const billingKey of [...new Set(billingKeys)].slice(0, 20)) {
			const rows = this.sql
				.exec(
					"SELECT source_billing, artist_mbid FROM festival_sets WHERE festival_id = ? AND billing_key = ? LIMIT 1",
					festivalId,
					billingKey,
				)
				.toArray() as unknown as Array<{ source_billing: string; artist_mbid: string | null }>;
			const row = rows[0];
			if (!row) continue;
			const identity = parseArtistBilling(
				row.source_billing,
				row.artist_mbid ?? undefined,
			).identityHint;
			const identityKey = (await artistIdentitySearchCacheKeys(identity))[0];
			if (identityKey) {
				this.sql.exec(
					"DELETE FROM artist_search_provider_attempts WHERE identity_key = ?",
					identityKey,
				);
			}
		}
	}

	getCachedArtistBillingResolution(billingKey: string): ArtistBillingResolution | null {
		const rows = this.sql
			.exec(
				"SELECT resolution_json FROM artist_billing_resolutions WHERE billing_key = ? LIMIT 1",
				billingKey,
			)
			.toArray() as unknown as Array<{ resolution_json: string }>;
		return rows[0]
			? parseStoredJson<ArtistBillingResolution>(
					rows[0].resolution_json,
					`artist billing resolution ${billingKey}`,
				)
			: null;
	}

	getCachedArtistResolutionSearch(cacheKey: string): unknown | null {
		const rows = this.sql
			.exec(
				"SELECT response_json FROM artist_resolution_search_cache WHERE cache_key = ? LIMIT 1",
				cacheKey,
			)
			.toArray() as unknown as Array<{ response_json: string }>;
		return rows[0]
			? parseStoredJson<unknown>(rows[0].response_json, `artist search cache ${cacheKey}`)
			: null;
	}

	putCachedArtistResolutionSearch(
		cacheKey: string,
		response: unknown,
		provider: "brave" | "exhausted" | "tavily" = "tavily",
	) {
		this.sql.exec(
			`INSERT OR REPLACE INTO artist_resolution_search_cache
			 (cache_key, provider, response_json, retrieved_at) VALUES (?, ?, ?, ?)`,
			cacheKey,
			provider,
			JSON.stringify(response),
			Math.floor(Date.now() / 1000),
		);
	}

	deleteCachedArtistResolutionSearches(cacheKeys: string[]) {
		for (const cacheKey of cacheKeys.slice(0, 20)) {
			this.sql.exec(
				"DELETE FROM artist_resolution_search_cache WHERE cache_key = ? AND provider IN ('brave', 'tavily')",
				cacheKey,
			);
		}
		this.sql.exec(
			"DELETE FROM artist_resolution_search_cache WHERE provider = 'brave' AND retrieved_at < ?",
			Math.floor(Date.now() / 1000) - 15 * 60,
		);
	}

	getArtistSearchProviderAttempts(identityKeys: string[]): Record<string, number> {
		const attempts: Record<string, number> = {};
		for (const identityKey of [...new Set(identityKeys)].slice(0, 20)) {
			const rows = this.sql
				.exec(
					"SELECT attempt_mask FROM artist_search_provider_attempts WHERE identity_key = ? LIMIT 1",
					identityKey,
				)
				.toArray() as unknown as Array<{ attempt_mask: number }>;
			attempts[identityKey] = rows[0]?.attempt_mask ?? 0;
		}
		return attempts;
	}

	recordArtistSearchProviderAttempts(identityKeys: string[], provider: "brave" | "tavily") {
		const providerMask = provider === "brave" ? 1 : 2;
		const now = Math.floor(Date.now() / 1000);
		for (const identityKey of [...new Set(identityKeys)].slice(0, 20)) {
			this.sql.exec(
				`INSERT INTO artist_search_provider_attempts (identity_key, attempt_mask, updated_at)
				 VALUES (?, ?, ?)
				 ON CONFLICT(identity_key) DO UPDATE SET
				   attempt_mask = attempt_mask | excluded.attempt_mask,
				   updated_at = excluded.updated_at`,
				identityKey,
				providerMask,
				now,
			);
		}
	}

	getCachedArtistResolutionAi(cacheKey: string): unknown | null {
		const rows = this.sql
			.exec(
				"SELECT response_json FROM artist_resolution_ai_cache WHERE cache_key = ? LIMIT 1",
				cacheKey,
			)
			.toArray() as unknown as Array<{ response_json: string }>;
		return rows[0]
			? parseStoredJson<unknown>(rows[0].response_json, `artist AI cache ${cacheKey}`)
			: null;
	}

	putCachedArtistResolutionAi(cacheKey: string, model: string, response: unknown) {
		this.sql.exec(
			`INSERT OR REPLACE INTO artist_resolution_ai_cache
			 (cache_key, model, response_json, created_at) VALUES (?, ?, ?, ?)`,
			cacheKey,
			model,
			JSON.stringify(response),
			Math.floor(Date.now() / 1000),
		);
	}

	recordArtistBillingResolution(resolution: ArtistBillingResolution): ArtistBillingResolution {
		const existing = this.getCachedArtistBillingResolution(resolution.billingKey);
		if (existing?.method === "manual") return existing;
		if (existing?.status === "resolved" && resolution.status !== "resolved") return existing;
		let stored = resolution;
		if (existing) {
			const unchanged =
				existing.inputHash === resolution.inputHash &&
				existing.processorVersion === resolution.processorVersion;
			stored = {
				...resolution,
				id: existing.id,
				version: unchanged ? existing.version : existing.version + 1,
			};
		}
		this.sql.exec(
			`INSERT OR REPLACE INTO artist_billing_resolutions
			 (id, billing_key, source_billing, status, method, confidence, resolution_json,
			  input_hash, processor_version, model, version, updated_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			stored.id,
			stored.billingKey,
			stored.sourceBilling,
			stored.status,
			stored.method,
			stored.confidence,
			JSON.stringify(stored),
			stored.inputHash,
			stored.processorVersion,
			stored.model ?? null,
			stored.version,
			Math.floor(Date.now() / 1000),
		);
		return stored;
	}

	applyArtistBillingResolution(
		festivalId: string,
		resolution: ArtistBillingResolution,
		profiles: ArtistProfile[],
	): { resolution: ArtistBillingResolution; profiles: ArtistProfile[]; setIds: string[] } | null {
		let effectiveResolution = resolution;
		let effectiveProfiles = profiles;
		const existingResolution = this.getCachedArtistBillingResolution(resolution.billingKey);
		if (resolution.method !== "manual") {
			const overrideRows = this.sql
				.exec(
					`SELECT resolution_json FROM artist_billing_overrides
					 WHERE billing_key = ? AND ((scope = 'festival' AND scope_id = ?) OR scope = 'global')
					 ORDER BY CASE scope WHEN 'festival' THEN 0 ELSE 1 END LIMIT 1`,
					resolution.billingKey,
					festivalId,
				)
				.toArray() as unknown as Array<{ resolution_json: string }>;
			let manual: ArtistBillingResolution | null = null;
			if (overrideRows[0]) {
				manual = parseStoredJson<ArtistBillingResolution>(
					overrideRows[0].resolution_json,
					`artist billing override ${resolution.billingKey}`,
				);
			} else if (existingResolution?.method === "manual") {
				manual = existingResolution;
			}
			if (manual) {
				effectiveResolution = manual;
				effectiveProfiles = this.getCanonicalArtistProfiles(
					manual.credits.map((credit) => credit.artistId),
				);
			} else if (existingResolution) {
				if (
					existingResolution.inputHash === resolution.inputHash &&
					existingResolution.processorVersion === resolution.processorVersion
				) {
					effectiveResolution = existingResolution;
					const suppliedIds = new Set(profiles.map((profile) => profile.id));
					effectiveProfiles = existingResolution.credits.every((credit) =>
						suppliedIds.has(credit.artistId),
					)
						? profiles
						: this.getCanonicalArtistProfiles(
								existingResolution.credits.map((credit) => credit.artistId),
							);
				} else {
					effectiveResolution = {
						...resolution,
						id: existingResolution.id,
						version: existingResolution.version + 1,
					};
				}
			}
		}
		if (effectiveResolution.status !== "resolved" || effectiveResolution.credits.length === 0) {
			return null;
		}
		effectiveProfiles = effectiveProfiles.map((profile) =>
			this.#mergeCanonicalArtistProfile(profile),
		);
		const profileById = new Map(effectiveProfiles.map((profile) => [profile.id, profile]));
		if (effectiveResolution.credits.some((credit) => !profileById.has(credit.artistId))) {
			throw new Error("artist resolution is missing a canonical profile");
		}
		const setRows = this.sql
			.exec(
				"SELECT id FROM festival_sets WHERE festival_id = ? AND billing_key = ? ORDER BY id",
				festivalId,
				effectiveResolution.billingKey,
			)
			.toArray() as unknown as Array<{ id: string }>;
		if (setRows.length === 0) return null;
		const now = Math.floor(Date.now() / 1000);
		this.ctx.storage.transactionSync(() => {
			for (const profile of effectiveProfiles) {
				const sourceKey = artistEnrichmentSourceKey(profile.name, profile.mbid);
				this.sql.exec(
					`INSERT OR REPLACE INTO canonical_artists
					 (id, source_key, profile_json, updated_at) VALUES (?, ?, ?, ?)`,
					profile.id,
					sourceKey,
					JSON.stringify(profile),
					now,
				);
				this.#indexCanonicalArtist(profile, sourceKey, now);
			}
			this.sql.exec(
				`INSERT OR REPLACE INTO artist_billing_resolutions
				 (id, billing_key, source_billing, status, method, confidence, resolution_json,
				  input_hash, processor_version, model, version, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
				effectiveResolution.id,
				effectiveResolution.billingKey,
				effectiveResolution.sourceBilling,
				effectiveResolution.status,
				effectiveResolution.method,
				effectiveResolution.confidence,
				JSON.stringify(effectiveResolution),
				effectiveResolution.inputHash,
				effectiveResolution.processorVersion,
				effectiveResolution.model ?? null,
				effectiveResolution.version,
				now,
			);
			this.#applyResolutionToFestivalSets(festivalId, effectiveResolution);
		});
		return {
			resolution: effectiveResolution,
			profiles: effectiveProfiles,
			setIds: setRows.map((row) => row.id),
		};
	}

	putGlobalArtistBillingOverride(
		festivalId: string,
		input: ArtistOverrideInput,
	): {
		resolution: ArtistBillingResolution;
		profiles: ArtistProfile[];
		applications: Array<{ festivalId: string; setIds: string[] }>;
	} {
		if (!input.billingKey || input.billingKey.length > 500) {
			throw new Error("artist override billing key is invalid");
		}
		if (input.credits.length === 0 || input.credits.length > 10) {
			throw new Error("artist override must contain 1-10 credits");
		}
		const setRows = this.sql
			.exec(
				`SELECT source_billing FROM festival_sets
				 WHERE festival_id = ? AND billing_key = ? ORDER BY id`,
				festivalId,
				input.billingKey,
			)
			.toArray() as unknown as Array<{ source_billing: string }>;
		if (setRows.length === 0) throw new Error("artist override billing was not found");
		const sourceBilling = setRows[0].source_billing;
		const normalizedSource = normalizeArtistBilling(sourceBilling);
		const artistIds = input.credits.map((credit) => credit.artistId);
		if (new Set(artistIds).size !== artistIds.length) {
			throw new Error("artist override contains duplicate credits");
		}
		for (const credit of input.credits) {
			if (
				!credit.artistId ||
				credit.artistId.length > 200 ||
				!credit.creditedAs ||
				credit.creditedAs.length > 200 ||
				!normalizedSource.includes(normalizeArtistBilling(credit.creditedAs)) ||
				!(["performer", "presenter", "guest"] as const).includes(credit.role)
			) {
				throw new Error("artist override credit is invalid");
			}
		}
		if (
			input.presentedTitle &&
			(input.presentedTitle.length > 200 ||
				!normalizedSource.includes(normalizeArtistBilling(input.presentedTitle)))
		) {
			throw new Error("artist override presented title is invalid");
		}
		const allowedQualifiers: PerformanceQualifier[] = [
			"dj_set",
			"live",
			"ambient_set",
			"hybrid_set",
			"reggae_set",
			"balearic_set",
			"electro_set",
			"r_and_b_set",
			"solo_piano",
			"live_keyboard",
		];
		if (input.performanceQualifiers?.some((qualifier) => !allowedQualifiers.includes(qualifier))) {
			throw new Error("artist override performance qualifier is invalid");
		}
		const profiles = this.getCanonicalArtistProfiles(artistIds);
		if (profiles.length !== artistIds.length) {
			throw new Error("artist override references an unknown canonical artist");
		}
		const profileById = new Map(profiles.map((profile) => [profile.id, profile]));
		const credits: ArtistCredit[] = input.credits.map((credit) => ({
			artistId: credit.artistId,
			canonicalName: profileById.get(credit.artistId)?.name ?? credit.creditedAs,
			creditedAs: credit.creditedAs,
			role: credit.role,
		}));
		const current = this.getCachedArtistBillingResolution(input.billingKey);
		const version = (current?.version ?? 0) + 1;
		const parsed = parseArtistBilling(sourceBilling);
		const resolution: ArtistBillingResolution = {
			id: current?.id ?? stableResolutionId(input.billingKey),
			sourceBilling,
			billingKey: input.billingKey,
			status: "resolved",
			method: "manual",
			confidence: 1,
			credits,
			...((input.presentedTitle ?? parsed.presentedTitle)
				? { presentedTitle: input.presentedTitle ?? parsed.presentedTitle }
				: {}),
			performanceQualifiers: input.performanceQualifiers ?? parsed.performanceQualifiers,
			evidence: [],
			inputHash: `manual:${version}:${input.billingKey}`,
			processorVersion: "manual-v1",
			version,
		};
		this.sql.exec(
			`INSERT OR REPLACE INTO artist_billing_overrides
			 (scope, scope_id, billing_key, resolution_json, revision, updated_at)
			 VALUES ('global', '*', ?, ?, ?, ?)`,
			input.billingKey,
			JSON.stringify(resolution),
			version,
			Math.floor(Date.now() / 1000),
		);
		const festivalRows = this.sql
			.exec(
				`SELECT DISTINCT festival_id FROM festival_sets
				 WHERE billing_key = ? ORDER BY festival_id`,
				input.billingKey,
			)
			.toArray() as unknown as Array<{ festival_id: string }>;
		const applications: Array<{ festivalId: string; setIds: string[] }> = [];
		for (const row of festivalRows) {
			const applied = this.applyArtistBillingResolution(row.festival_id, resolution, profiles);
			if (applied) applications.push({ festivalId: row.festival_id, setIds: applied.setIds });
		}
		if (applications.length === 0) throw new Error("artist override could not be applied");
		return { resolution, profiles, applications };
	}

	listArtistResolutionApplications(festivalId: string): Array<{
		billingKey: string;
		resolutionId: string;
		resolutionVersion: number;
		appliedAt: number | null;
	}> {
		return (
			this.sql
				.exec(
					`SELECT billing_key, resolution_id, resolution_version, applied_at
					 FROM festival_resolution_applications
					 WHERE festival_id = ? ORDER BY billing_key`,
					festivalId,
				)
				.toArray() as unknown as Array<{
				billing_key: string;
				resolution_id: string;
				resolution_version: number;
				applied_at: number | null;
			}>
		).map((row) => ({
			billingKey: row.billing_key,
			resolutionId: row.resolution_id,
			resolutionVersion: row.resolution_version,
			appliedAt: row.applied_at,
		}));
	}

	listArtistBillingResolutions(festivalId: string): ArtistBillingResolution[] {
		const rows = this.sql
			.exec(
				`SELECT DISTINCT r.resolution_json FROM artist_billing_resolutions r
				 WHERE r.billing_key IN (
				   SELECT billing_key FROM festival_sets WHERE festival_id = ?
				 ) ORDER BY r.source_billing`,
				festivalId,
			)
			.toArray() as unknown as Array<{ resolution_json: string }>;
		return rows.map((row) =>
			parseStoredJson<ArtistBillingResolution>(
				row.resolution_json,
				"festival artist billing resolution",
			),
		);
	}

	markArtistResolutionComplete(jobId: string, status: string) {
		this.sql.exec(
			`UPDATE artist_enrichment_jobs
			 SET status = ?, attempts = attempts + 1, last_error = NULL, updated_at = ? WHERE id = ?`,
			status.slice(0, 40),
			Math.floor(Date.now() / 1000),
			jobId,
		);
	}

	markArtistResolutionApplied(
		festivalId: string,
		billingKey: string,
		resolutionId: string,
		resolutionVersion: number,
	) {
		const now = Math.floor(Date.now() / 1000);
		this.sql.exec(
			`UPDATE festival_resolution_applications
			 SET applied_at = ?, updated_at = ?
			 WHERE festival_id = ? AND billing_key = ? AND resolution_id = ?
			   AND resolution_version = ?`,
			now,
			now,
			festivalId,
			billingKey,
			resolutionId,
			resolutionVersion,
		);
	}

	markArtistEnrichmentFailure(jobId: string, error: string) {
		this.sql.exec(
			`UPDATE artist_enrichment_jobs
			 SET status = 'failed', attempts = attempts + 1, last_error = ?, updated_at = ? WHERE id = ?`,
			error.slice(0, 500),
			Math.floor(Date.now() / 1000),
			jobId,
		);
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
		const url = requireUrl(request.url);
		const path = url.pathname;
		const method = request.method;
		const env = this.env as Record<string, unknown>;

		// GET /festivals
		if (method === "GET" && path === "/festivals") {
			return Response.json(this.#getFestivals());
		}
		if (method === "POST" && path === "/artist-identities/search") {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			try {
				const input = parseStoredJson<{ query: string; limit?: number }>(
					await request.text(),
					"artist identity search",
				);
				if (typeof input.query !== "string" || input.query.length > 500) {
					return new Response("Invalid artist identity search", { status: 400 });
				}
				return Response.json(this.searchCanonicalArtistProfiles(input.query, input.limit));
			} catch {
				return new Response("Invalid artist identity search", { status: 400 });
			}
		}
		if (method === "POST" && path === "/artist-identities/merge") {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			let input: ArtistIdentityMergeInput;
			try {
				input = parseStoredJson<ArtistIdentityMergeInput>(
					await request.text(),
					"artist identity merge",
				);
			} catch {
				return new Response("Invalid artist identity merge", { status: 400 });
			}
			try {
				return Response.json(this.mergeAdminArtistIdentity(input));
			} catch (error) {
				console.error("Artist identity merge rejected", error);
				return new Response("Artist identity merge was rejected", { status: 400 });
			}
		}
		if (method === "PUT" && path === "/artist-identities") {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			let input: ArtistIdentityInput;
			try {
				const rawBody = await request.text();
				input = parseStoredJson<ArtistIdentityInput>(rawBody, "artist identity");
			} catch {
				return new Response("Invalid artist identity", { status: 400 });
			}
			try {
				return Response.json(await this.putAdminArtistIdentity(input));
			} catch (error) {
				console.error("Artist identity rejected", error);
				return new Response("Artist identity was rejected", { status: 400 });
			}
		}
		if (method === "POST" && path === "/artist-resolutions/backfill") {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			return Response.json({
				festivalIds: this.#getFestivals().map((festival) => String(festival.id)),
			});
		}

		// GET /festivals/:id/lineup
		const lineupMatch = path.match(/^\/festivals\/([^/]+)\/lineup$/);
		if (method === "GET" && lineupMatch) {
			const id = lineupMatch[1];
			const lineup = this.#getLineup(id);
			if (!lineup) return new Response("Festival not found", { status: 404 });
			return Response.json(lineup);
		}

		// Admin review and durable global overrides for artist billing resolution.
		const artistResolutionRetryMatch = path.match(
			/^\/festivals\/([^/]+)\/artist-resolutions\/retry$/,
		);
		if (method === "POST" && artistResolutionRetryMatch) {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			const festivalId = artistResolutionRetryMatch[1];
			if (!this.#getFestival(festivalId)) {
				return new Response("Festival not found", { status: 404 });
			}
			let billingKeys: string[] = [];
			try {
				const rawBody = await request.text();
				if (rawBody) {
					const body = parseStoredJson<{ billingKeys?: unknown }>(rawBody, "artist retry");
					if (
						body.billingKeys !== undefined &&
						(!Array.isArray(body.billingKeys) ||
							body.billingKeys.length > 20 ||
							!body.billingKeys.every(
								(key): key is string => typeof key === "string" && key.startsWith("name:"),
							))
					) {
						return new Response("Invalid billing keys", { status: 400 });
					}
					billingKeys = [...new Set(body.billingKeys ?? [])];
				}
			} catch {
				return new Response("Invalid artist retry", { status: 400 });
			}
			if (billingKeys.length > 0) {
				await this.resetArtistSearchProviderAttemptsForBillings(festivalId, billingKeys);
			}
			return Response.json({ ok: true, billingKeys });
		}
		const artistApplicationsMatch = path.match(
			/^\/festivals\/([^/]+)\/artist-resolution-applications$/,
		);
		if (method === "GET" && artistApplicationsMatch) {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			const festivalId = artistApplicationsMatch[1];
			if (!this.#getFestival(festivalId)) {
				return new Response("Festival not found", { status: 404 });
			}
			return Response.json(this.listArtistResolutionApplications(festivalId));
		}
		const artistResolutionMatch = path.match(/^\/festivals\/([^/]+)\/artist-resolutions$/);
		if (artistResolutionMatch && (method === "GET" || method === "PUT")) {
			const authResult = await this.#requireArtistResolutionAdmin(request);
			if (authResult instanceof Response) return authResult;
			const festivalId = artistResolutionMatch[1];
			if (!this.#getFestival(festivalId)) {
				return new Response("Festival not found", { status: 404 });
			}
			if (method === "GET") {
				return Response.json(this.listArtistBillingResolutions(festivalId));
			}
			const rawBody = await request.text();
			if (new TextEncoder().encode(rawBody).byteLength > MAX_IMPORT_REQUEST_BYTES) {
				return new Response("Artist override is too large", { status: 413 });
			}
			let input: ArtistOverrideInput;
			try {
				const candidate = parseStoredJson<Record<string, unknown>>(rawBody, "artist override");
				if (!Array.isArray(candidate.credits)) throw new Error("credits are required");
				input = candidate as unknown as ArtistOverrideInput;
			} catch {
				return new Response("Invalid artist override", { status: 400 });
			}
			try {
				return Response.json(this.putGlobalArtistBillingOverride(festivalId, input));
			} catch (error) {
				console.error("Artist billing override rejected", error);
				return new Response("Artist override was rejected", { status: 400 });
			}
		}

		// GET /festivals/:id
		const festivalMatch = path.match(/^\/festivals\/([^/]+)$/);
		if (method === "GET" && festivalMatch) {
			const id = festivalMatch[1];
			const festival = this.#getFestival(id);
			if (!festival) return new Response("Festival not found", { status: 404 });
			return Response.json(festival);
		}

		// POST /festival-imports/preview — registered users validate a Clashfinder source.
		if (method === "POST" && path === "/festival-imports/preview") {
			const rawBody = await request.text();
			if (new TextEncoder().encode(rawBody).byteLength > MAX_IMPORT_REQUEST_BYTES) {
				return new Response("Import request is too large", { status: 413 });
			}
			const user = await this.#requireRegisteredImportUser(request, rawBody);
			if (user instanceof Response) return user;
			const attempt = await this.#beginImportAttempt(
				request,
				user.publicKey,
				user.nonce,
				"preview",
				null,
			);
			if (attempt instanceof Response) return attempt;
			let body: { clashfinder?: string } | null;
			try {
				body = parseStoredJson<{ clashfinder: string }>(rawBody, "festival import preview request");
			} catch {
				this.#finishImportAttempt(attempt, "invalid_json");
				return new Response("Invalid import request JSON", { status: 400 });
			}
			const clashfinderId = normalizeClashfinderId(body?.clashfinder ?? "");
			if (!clashfinderId) {
				this.#finishImportAttempt(attempt, "invalid_source");
				return new Response("Enter a valid Clashfinder URL or event ID", { status: 400 });
			}
			this.#cleanupFestivalImports();
			const existing = this.#getFestivalByClashfinderId(clashfinderId);
			if (existing) {
				this.#finishImportAttempt(attempt, "existing");
				return Response.json({ status: "existing", festival: existing });
			}

			try {
				const apiResponse = await this.#fetchClashfinder(clashfinderId);
				const validated = validateClashfinderImport(clashfinderId, apiResponse);
				const previewId = crypto.randomUUID();
				const now = Math.floor(Date.now() / 1000);
				const expiresAt = now + IMPORT_PREVIEW_TTL_SECONDS;
				this.sql.exec(
					`INSERT INTO festival_import_previews
					 (id, public_key, clashfinder_id, festival_id, name, start_date, end_date, year,
					  stage_count, set_count, lineup_json, expires_at, created_at)
					 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
					previewId,
					user.publicKey,
					validated.clashfinderId,
					validated.festivalId,
					validated.name,
					validated.startDate,
					validated.endDate,
					validated.year,
					validated.stageCount,
					validated.setCount,
					JSON.stringify(validated.lineup),
					expiresAt,
					now,
				);
				this.#finishImportAttempt(attempt, "previewed");
				return Response.json({
					status: "preview",
					preview: {
						id: previewId,
						clashfinderId: validated.clashfinderId,
						name: validated.name,
						startDate: validated.startDate,
						endDate: validated.endDate,
						stageCount: validated.stageCount,
						setCount: validated.setCount,
						expiresAt: new Date(expiresAt * 1000).toISOString(),
					},
				});
			} catch (error) {
				this.#finishImportAttempt(attempt, "rejected");
				console.warn("Festival import preview rejected", error);
				return new Response("Clashfinder event could not be imported", { status: 422 });
			}
		}

		// POST /festival-imports/:previewId/publish — publish the validated preview.
		const importPublishMatch = path.match(
			/^\/festival-imports\/([0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12})\/publish$/i,
		);
		if (method === "POST" && importPublishMatch) {
			const rawBody = await request.text();
			if (new TextEncoder().encode(rawBody).byteLength > MAX_IMPORT_REQUEST_BYTES) {
				return new Response("Import request is too large", { status: 413 });
			}
			const user = await this.#requireRegisteredImportUser(request, rawBody);
			if (user instanceof Response) return user;
			this.#cleanupFestivalImports();
			const completedRows = this.sql
				.exec(
					"SELECT festival_id FROM festival_import_results WHERE preview_id = ? AND public_key = ? AND expires_at >= ? LIMIT 1",
					importPublishMatch[1],
					user.publicKey,
					Math.floor(Date.now() / 1000),
				)
				.toArray() as Array<{ festival_id: string }>;
			if (completedRows[0]) {
				const retryAttempt = await this.#beginImportAttempt(
					request,
					user.publicKey,
					user.nonce,
					"retry",
					null,
				);
				if (retryAttempt instanceof Response) return retryAttempt;
				this.#finishImportAttempt(retryAttempt, "retry");
				return Response.json({
					status: "existing",
					festival: this.#getFestival(completedRows[0].festival_id),
				});
			}
			const attempt = await this.#beginImportAttempt(
				request,
				user.publicKey,
				user.nonce,
				"publish",
				null,
			);
			if (attempt instanceof Response) return attempt;
			let body: { name?: string; location?: string; city?: string; country?: string } | null;
			try {
				body = parseStoredJson<typeof body>(rawBody, "festival import publish request");
			} catch {
				this.#finishImportAttempt(attempt, "invalid_json");
				return new Response("Invalid import request JSON", { status: 400 });
			}
			const name = body?.name?.trim();
			const location = body?.location?.trim();
			const city = body?.city?.trim();
			const country = body?.country?.trim().toUpperCase();
			if (
				!name ||
				name.length > 200 ||
				!location ||
				location.length > 200 ||
				!city ||
				city.length > 120 ||
				!country ||
				!/^[A-Z]{2}$/.test(country)
			) {
				this.#finishImportAttempt(attempt, "invalid_metadata");
				return new Response("Name, venue, city, and two-letter country code are required", {
					status: 400,
				});
			}

			const previewRows = this.sql
				.exec(
					"SELECT * FROM festival_import_previews WHERE id = ? AND public_key = ? AND expires_at >= ? LIMIT 1",
					importPublishMatch[1],
					user.publicKey,
					Math.floor(Date.now() / 1000),
				)
				.toArray() as Array<{
				id: string;
				clashfinder_id: string;
				festival_id: string;
				start_date: string;
				end_date: string;
				year: number;
				lineup_json: string;
			}>;
			const preview = previewRows[0];
			if (!preview) {
				this.#finishImportAttempt(attempt, "missing_preview");
				return new Response("Import preview expired or not found", { status: 404 });
			}
			const existing = this.#getFestivalByClashfinderId(preview.clashfinder_id);
			if (existing) {
				this.ctx.storage.transactionSync(() => {
					this.sql.exec(
						"INSERT OR REPLACE INTO festival_import_results (preview_id, public_key, festival_id, expires_at) VALUES (?, ?, ?, ?)",
						preview.id,
						user.publicKey,
						existing.id,
						Math.floor(Date.now() / 1000) + IMPORT_PREVIEW_TTL_SECONDS,
					);
					this.sql.exec("DELETE FROM festival_import_previews WHERE id = ?", preview.id);
					this.#finishImportAttempt(attempt, "existing");
				});
				return Response.json({ status: "existing", festival: existing });
			}
			if (this.#getFestival(preview.festival_id)) {
				this.#finishImportAttempt(attempt, "id_collision");
				return new Response("Generated festival ID already exists", { status: 409 });
			}

			try {
				const lineup = parseStoredJson<Lineup>(preview.lineup_json, `import preview ${preview.id}`);
				lineup.festival = { id: preview.festival_id, name, location };
				const today = new Date().toISOString().split("T")[0];
				const status =
					today < preview.start_date ? "upcoming" : today > preview.end_date ? "past" : "live";
				this.ctx.storage.transactionSync(() => {
					this.sql.exec(
						`INSERT INTO festivals
						 (id, name, year, location, city, country, start_date, end_date, genres, status, clashfinder_id)
						 VALUES (?, ?, ?, ?, ?, ?, ?, ?, '[]', ?, ?)`,
						preview.festival_id,
						name,
						preview.year,
						location,
						city,
						country,
						preview.start_date,
						preview.end_date,
						status,
						preview.clashfinder_id,
					);
					this.#upsertLineup(preview.festival_id, lineup);
					this.sql.exec(
						"INSERT INTO festival_import_results (preview_id, public_key, festival_id, expires_at) VALUES (?, ?, ?, ?)",
						preview.id,
						user.publicKey,
						preview.festival_id,
						Math.floor(Date.now() / 1000) + IMPORT_PREVIEW_TTL_SECONDS,
					);
					this.sql.exec("DELETE FROM festival_import_previews WHERE id = ?", preview.id);
					this.#finishImportAttempt(attempt, "published");
				});
				return Response.json(
					{
						status: "created",
						festival: this.#getFestival(preview.festival_id),
						lineup: this.#getLineup(preview.festival_id),
					},
					{ status: 201 },
				);
			} catch (error) {
				this.#finishImportAttempt(attempt, "failed");
				console.error("Festival import publish failed", error);
				return new Response("Festival could not be published", { status: 500 });
			}
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

			// Fetch lineup from Clashfinder API, or from a local fixture in CI/dev tests.
			let apiResponse: ClashfinderApiResponse;
			try {
				apiResponse = await this.#fetchClashfinder(src.clashfinderId);
			} catch (error) {
				if (error instanceof Error && error.message === "Clashfinder credentials not configured") {
					return new Response(error.message, { status: 500 });
				}
				throw error;
			}
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
			const current = this.#getFestival(id) as Record<string, unknown> | null;
			if (!current) {
				return new Response("Festival not found", { status: 404 });
			}

			const body = (await request.json()) as Record<string, unknown>;
			const value = (key: string) => (body[key] === undefined ? current[key] : body[key]);
			this.sql.exec(
				`UPDATE festivals SET
					name = ?, year = ?, location = ?, city = ?, country = ?,
					start_date = ?, end_date = ?, status = ?, genres = ?, lat = ?, lon = ?,
					updated_at = datetime('now')
				 WHERE id = ?`,
				value("name"),
				value("year"),
				value("location"),
				value("city"),
				value("country"),
				value("startDate"),
				value("endDate"),
				value("status"),
				JSON.stringify(value("genres")),
				value("lat") ?? null,
				value("lon") ?? null,
				id,
			);

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

			// Delete festival-owned data first (foreign key constraints).
			this.sql.exec("DELETE FROM artist_enrichment_jobs WHERE festival_id = ?", id);
			this.sql.exec("DELETE FROM festival_resolution_applications WHERE festival_id = ?", id);
			this.sql.exec(
				"DELETE FROM artist_billing_overrides WHERE scope = 'festival' AND scope_id = ?",
				id,
			);
			this.sql.exec("DELETE FROM festival_artists WHERE festival_id = ?", id);
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

			// Fetch updated lineup from Clashfinder API, or from a local fixture in CI/dev tests.
			let apiResponse: ClashfinderApiResponse;
			try {
				apiResponse = await this.#fetchClashfinder(clashfinderId);
			} catch (error) {
				if (error instanceof Error && error.message === "Clashfinder credentials not configured") {
					return new Response(error.message, { status: 500 });
				}
				throw error;
			}
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

			// Verify this WebAuthn credential is bound to the requested Ed25519 key.
			if (credential.ed25519PublicKey !== body.ed25519PublicKey.toLowerCase()) {
				return new Response("Ed25519 key does not match registered credential", { status: 403 });
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

			// The authenticated WebAuthn credential must own this Ed25519 key.
			if (credential.ed25519PublicKey !== body.ed25519PublicKey.toLowerCase()) {
				return new Response("Ed25519 key does not match registered credential", { status: 403 });
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
