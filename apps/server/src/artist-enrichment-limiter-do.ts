import { DurableObject } from "cloudflare:workers";

const MUSICBRAINZ_MIN_INTERVAL_MS = 1_100;

export class ArtistEnrichmentLimiterDO extends DurableObject {
	get sql() {
		return this.ctx.storage.sql;
	}

	constructor(ctx: DurableObjectState, env: Record<string, unknown>) {
		super(ctx, env);
		this.sql.exec(`
			CREATE TABLE IF NOT EXISTS provider_rate_limit (
				provider TEXT PRIMARY KEY,
				next_allowed_at_ms INTEGER NOT NULL
			);
			CREATE TABLE IF NOT EXISTS provider_state (
				key TEXT PRIMARY KEY,
				value INTEGER NOT NULL
			)
		`);
	}

	nextArtistSearchProvider(braveAvailable: boolean, tavilyAvailable: boolean): "brave" | "tavily" {
		if (!braveAvailable && !tavilyAvailable)
			throw new Error("no artist search provider configured");
		if (!braveAvailable) return "tavily";
		if (!tavilyAvailable) return "brave";
		const rows = this.sql
			.exec("SELECT value FROM provider_state WHERE key = 'artist_search_next' LIMIT 1")
			.toArray() as unknown as Array<{ value: number }>;
		const provider = (rows[0]?.value ?? 0) % 2 === 0 ? "brave" : "tavily";
		this.sql.exec(
			`INSERT OR REPLACE INTO provider_state (key, value)
			 VALUES ('artist_search_next', ?)`,
			(rows[0]?.value ?? 0) + 1,
		);
		return provider;
	}

	reserveMusicBrainz(): number {
		const now = Date.now();
		const rows = this.sql
			.exec(
				"SELECT next_allowed_at_ms FROM provider_rate_limit WHERE provider = 'musicbrainz' LIMIT 1",
			)
			.toArray() as unknown as Array<{ next_allowed_at_ms: number }>;
		const reservedAt = Math.max(now, rows[0]?.next_allowed_at_ms ?? now);
		this.sql.exec(
			`INSERT OR REPLACE INTO provider_rate_limit (provider, next_allowed_at_ms)
			 VALUES ('musicbrainz', ?)`,
			reservedAt + MUSICBRAINZ_MIN_INTERVAL_MS,
		);
		return reservedAt - now;
	}
}
