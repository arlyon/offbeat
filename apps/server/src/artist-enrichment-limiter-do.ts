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
			)
		`);
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
