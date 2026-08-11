import app from "./api";
import {
	type ArtistEnrichmentQueueEnv,
	handleArtistEnrichmentQueue,
} from "./artist-enrichment-queue";

export { ArtistEnrichmentLimiterDO } from "./artist-enrichment-limiter-do";
export { FestivalDO } from "./festival-do";
export { MainDO } from "./main-do";

export default {
	fetch: app.fetch,
	queue: handleArtistEnrichmentQueue,
} satisfies ExportedHandler<ArtistEnrichmentQueueEnv>;
