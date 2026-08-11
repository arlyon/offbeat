import type { ArtistProfile } from "@offbeat/protocol";
import {
	type ArtistEnrichmentMessage,
	type ArtistEnrichmentOutcome,
	ArtistProviderError,
	enrichArtist,
} from "./artist-enrichment";

export interface ArtistEnrichmentQueueEnv {
	MAIN_DO: DurableObjectNamespace;
	FESTIVAL_DO: DurableObjectNamespace;
	ARTIST_ENRICHMENT_LIMITER: DurableObjectNamespace;
	MUSICBRAINZ_USER_AGENT: string;
}

interface MainArtistEnrichmentRpc {
	getCachedArtistEnrichment(sourceKey: string): Promise<ArtistEnrichmentOutcome | null>;
	applyArtistEnrichment(
		message: ArtistEnrichmentMessage,
		outcome: ArtistEnrichmentOutcome,
	): Promise<{ profile: ArtistProfile; setIds: string[] } | null>;
	markArtistEnrichmentFailure(jobId: string, error: string): Promise<void>;
}

interface FestivalArtistEnrichmentRpc {
	applyArtistEnrichment(
		festivalId: string,
		profile: ArtistProfile,
		setIds: string[],
	): Promise<void>;
}

interface ArtistEnrichmentLimiterRpc {
	reserveMusicBrainz(): Promise<number>;
}

export async function handleArtistEnrichmentQueue(
	batch: MessageBatch<unknown>,
	env: ArtistEnrichmentQueueEnv,
): Promise<void> {
	const mainId = env.MAIN_DO.idFromName("main");
	const main = env.MAIN_DO.get(mainId) as unknown as MainArtistEnrichmentRpc;
	for (const message of batch.messages) {
		const body = parseArtistEnrichmentMessage(message.body);
		if (!body) {
			message.ack();
			continue;
		}
		try {
			let outcome = await main.getCachedArtistEnrichment(body.sourceKey);
			if (!outcome) {
				const limiterId = env.ARTIST_ENRICHMENT_LIMITER.idFromName("musicbrainz");
				const limiter = env.ARTIST_ENRICHMENT_LIMITER.get(
					limiterId,
				) as unknown as ArtistEnrichmentLimiterRpc;
				outcome = await enrichArtist(body, {
					userAgent: env.MUSICBRAINZ_USER_AGENT,
					beforeMusicBrainzRequest: async () => {
						const delayMs = await limiter.reserveMusicBrainz();
						if (delayMs > 0) await wait(delayMs);
					},
				});
			}

			const applied = await main.applyArtistEnrichment(body, outcome);
			if (applied) {
				const festivalId = env.FESTIVAL_DO.idFromName(body.festivalId);
				const festival = env.FESTIVAL_DO.get(festivalId) as unknown as FestivalArtistEnrichmentRpc;
				await festival.applyArtistEnrichment(body.festivalId, applied.profile, applied.setIds);
			}
			message.ack();
		} catch (error) {
			const detail = error instanceof Error ? error.message : String(error);
			if (error instanceof ArtistProviderError && !error.retryable) {
				await main.applyArtistEnrichment(body, {
					status: "unresolved",
					reason: "provider_rejected",
				});
				message.ack();
				continue;
			}
			await main.markArtistEnrichmentFailure(body.jobId, detail);
			const delaySeconds = Math.min(900, 15 * 2 ** Math.min(message.attempts, 6));
			message.retry({ delaySeconds });
		}
	}
}

function parseArtistEnrichmentMessage(value: unknown): ArtistEnrichmentMessage | null {
	if (!value || typeof value !== "object") return null;
	const candidate = value as Record<string, unknown>;
	if (
		typeof candidate.jobId !== "string" ||
		candidate.jobId.length > 128 ||
		typeof candidate.sourceKey !== "string" ||
		candidate.sourceKey.length > 400 ||
		typeof candidate.festivalId !== "string" ||
		candidate.festivalId.length > 200 ||
		typeof candidate.billing !== "string" ||
		candidate.billing.length > 300 ||
		(candidate.mbid !== undefined &&
			(typeof candidate.mbid !== "string" || candidate.mbid.length > 100)) ||
		!Array.isArray(candidate.setIds) ||
		candidate.setIds.length > 500 ||
		!candidate.setIds.every((setId) => typeof setId === "string" && setId.length <= 200)
	) {
		return null;
	}
	return {
		jobId: candidate.jobId,
		sourceKey: candidate.sourceKey,
		festivalId: candidate.festivalId,
		billing: candidate.billing,
		setIds: candidate.setIds as string[],
		...(typeof candidate.mbid === "string" ? { mbid: candidate.mbid } : {}),
	};
}

function wait(milliseconds: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
