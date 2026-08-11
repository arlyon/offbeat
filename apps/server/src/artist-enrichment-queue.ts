import type {
	ArtistBillingResolution,
	ArtistCredit,
	ArtistCreditProposal,
	ArtistProfile,
	ArtistResolutionEvidence,
} from "@offbeat/protocol";
import { parseArtistBilling } from "@offbeat/protocol";
import {
	type ArtistEnrichmentMessage,
	type ArtistEnrichmentOutcome,
	ArtistProviderError,
	artistEnrichmentSourceKey,
	enrichArtist,
} from "./artist-enrichment";
import {
	ARTIST_RESOLUTION_MODEL,
	ARTIST_RESOLVER_VERSION,
	type ArtistResolutionInput,
	ArtistResolutionProviderError,
	type ArtistResolutionResult,
	artistResolutionCacheMaterial,
	resolveArtistBilling,
	sha256CanonicalJson,
} from "./artist-resolution";

export interface ArtistEnrichmentQueueEnv {
	MAIN_DO: DurableObjectNamespace;
	FESTIVAL_DO: DurableObjectNamespace;
	ARTIST_ENRICHMENT_LIMITER: DurableObjectNamespace;
	MUSICBRAINZ_USER_AGENT: string;
	AI_GATEWAY_BASE_URL?: string;
	AI_GATEWAY_TOKEN?: string;
	ARTIST_RESOLUTION_MODEL?: string;
	DEEPSEEK_API_KEY?: string;
	TAVILY_API_KEY?: string;
	DISABLE_ARTIST_RESOLUTION?: string;
}

interface ArtistResolutionApplication {
	resolution: ArtistBillingResolution;
	profiles: ArtistProfile[];
	setIds: string[];
}

interface MainArtistEnrichmentRpc {
	getCachedArtistEnrichment(sourceKey: string): Promise<ArtistEnrichmentOutcome | null>;
	cacheCanonicalArtistEnrichment(
		sourceKey: string,
		outcome: ArtistEnrichmentOutcome,
	): Promise<void>;
	getCachedArtistBillingResolution(billingKey: string): Promise<ArtistBillingResolution | null>;
	getCanonicalArtistProfiles(artistIds: string[]): Promise<ArtistProfile[]>;
	getCachedArtistResolutionSearch(cacheKey: string): Promise<unknown | null>;
	putCachedArtistResolutionSearch(cacheKey: string, response: unknown): Promise<void>;
	getCachedArtistResolutionAi(cacheKey: string): Promise<unknown | null>;
	putCachedArtistResolutionAi(cacheKey: string, model: string, response: unknown): Promise<void>;
	recordArtistBillingResolution(
		resolution: ArtistBillingResolution,
	): Promise<ArtistBillingResolution>;
	applyArtistBillingResolution(
		festivalId: string,
		resolution: ArtistBillingResolution,
		profiles: ArtistProfile[],
	): Promise<ArtistResolutionApplication | null>;
	markArtistResolutionComplete(jobId: string, status: string): Promise<void>;
	markArtistResolutionApplied(
		festivalId: string,
		billingKey: string,
		resolutionId: string,
		resolutionVersion: number,
	): Promise<void>;
	markArtistEnrichmentFailure(jobId: string, error: string): Promise<void>;
}

interface FestivalArtistResolutionRpc {
	applyArtistResolution(
		festivalId: string,
		resolution: ArtistBillingResolution,
		profiles: ArtistProfile[],
		setIds: string[],
	): Promise<"applied" | "already_applied" | "stale">;
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
	const limiterId = env.ARTIST_ENRICHMENT_LIMITER.idFromName("musicbrainz");
	const limiter = env.ARTIST_ENRICHMENT_LIMITER.get(
		limiterId,
	) as unknown as ArtistEnrichmentLimiterRpc;

	for (const message of batch.messages) {
		const body = parseArtistEnrichmentMessage(message.body);
		if (!body) {
			message.ack();
			continue;
		}
		try {
			const cachedResolution = await main.getCachedArtistBillingResolution(body.billingKey);
			if (cachedResolution?.status === "resolved") {
				await applyCachedResolution(env, main, body, cachedResolution);
				await main.markArtistResolutionComplete(body.jobId, cachedResolution.status);
				message.ack();
				continue;
			}
			if (cachedResolution && !artistResolutionConfigured(env)) {
				await main.markArtistResolutionComplete(body.jobId, cachedResolution.status);
				message.ack();
				continue;
			}

			const directOutcome = await enrichCandidate(main, limiter, env, body, false);
			if (directOutcome.status === "enriched") {
				const resolution = await directArtistResolution(body, directOutcome.profile);
				await publishResolution(env, main, body.festivalId, resolution, [directOutcome.profile]);
				await main.markArtistResolutionComplete(body.jobId, "resolved");
				message.ack();
				continue;
			}

			const input = resolutionInput(body);
			if (!artistResolutionConfigured(env)) {
				const unresolved = await unresolvedArtistResolution(body, input);
				await main.recordArtistBillingResolution(unresolved);
				await main.markArtistResolutionComplete(body.jobId, "unresolved");
				message.ack();
				continue;
			}

			const aiResult = await resolveArtistBilling(input, {
				tavilyApiKey: env.TAVILY_API_KEY ?? "",
				deepSeekApiKey: env.DEEPSEEK_API_KEY ?? "",
				gatewayBaseUrl: env.AI_GATEWAY_BASE_URL ?? "",
				gatewayToken: env.AI_GATEWAY_TOKEN ?? "",
				cache: {
					getSearch: (cacheKey) => main.getCachedArtistResolutionSearch(cacheKey),
					putSearch: (cacheKey, response) =>
						main.putCachedArtistResolutionSearch(cacheKey, response),
					getAi: (cacheKey) => main.getCachedArtistResolutionAi(cacheKey),
					putAi: (cacheKey, response) =>
						main.putCachedArtistResolutionAi(cacheKey, ARTIST_RESOLUTION_MODEL, response),
				},
			});
			const inputHash = await resolutionInputHash(input);
			let resolution = protocolResolution(body, aiResult, inputHash);
			if (aiResult.status !== "resolved") {
				resolution = await main.recordArtistBillingResolution(resolution);
				await main.markArtistResolutionComplete(body.jobId, resolution.status);
				message.ack();
				continue;
			}

			const enriched = await enrichResolutionCredits(main, limiter, env, body, aiResult);
			if (!enriched) {
				resolution = {
					...resolution,
					status: "needs_review",
					credits: [],
				};
				await main.recordArtistBillingResolution(resolution);
				await main.markArtistResolutionComplete(body.jobId, "needs_review");
				message.ack();
				continue;
			}
			resolution = { ...resolution, credits: enriched.credits };
			await publishResolution(env, main, body.festivalId, resolution, enriched.profiles);
			await main.markArtistResolutionComplete(body.jobId, "resolved");
			message.ack();
		} catch (error) {
			const detail = error instanceof Error ? error.message : String(error);
			const nonRetryableProviderError =
				(error instanceof ArtistProviderError && !error.retryable) ||
				(error instanceof ArtistResolutionProviderError && !error.retryable);
			if (nonRetryableProviderError) {
				const input = resolutionInput(body);
				const unresolved = await unresolvedArtistResolution(body, input);
				await main.recordArtistBillingResolution(unresolved);
				await main.markArtistResolutionComplete(body.jobId, "unresolved");
				message.ack();
				continue;
			}
			await main.markArtistEnrichmentFailure(body.jobId, detail);
			const delaySeconds = Math.min(900, 15 * 2 ** Math.min(message.attempts, 6));
			message.retry({ delaySeconds });
		}
	}
}

async function enrichCandidate(
	main: MainArtistEnrichmentRpc,
	limiter: ArtistEnrichmentLimiterRpc,
	env: ArtistEnrichmentQueueEnv,
	candidate: ArtistEnrichmentMessage,
	allowAmbiguousBilling: boolean,
): Promise<ArtistEnrichmentOutcome> {
	const sourceKey = artistEnrichmentSourceKey(candidate.billing, candidate.mbid);
	let outcome = await main.getCachedArtistEnrichment(sourceKey);
	if (!outcome) {
		outcome = await enrichArtist(candidate, {
			userAgent: env.MUSICBRAINZ_USER_AGENT,
			allowAmbiguousBilling,
			beforeMusicBrainzRequest: async () => {
				const delayMs = await limiter.reserveMusicBrainz();
				if (delayMs > 0) await wait(delayMs);
			},
		});
		await main.cacheCanonicalArtistEnrichment(sourceKey, outcome);
	}
	return outcome;
}

async function enrichResolutionCredits(
	main: MainArtistEnrichmentRpc,
	limiter: ArtistEnrichmentLimiterRpc,
	env: ArtistEnrichmentQueueEnv,
	body: ArtistEnrichmentMessage,
	result: ArtistResolutionResult,
): Promise<{ credits: ArtistCredit[]; profiles: ArtistProfile[] } | null> {
	const credits: ArtistCredit[] = [];
	const profiles = new Map<string, ArtistProfile>();
	for (const proposal of result.credits) {
		const candidate: ArtistEnrichmentMessage = {
			...body,
			billing: proposal.canonicalName,
			sourceKey: artistEnrichmentSourceKey(proposal.canonicalName),
		};
		const outcome = await enrichCandidate(main, limiter, env, candidate, true);
		if (outcome.status !== "enriched") return null;
		profiles.set(outcome.profile.id, outcome.profile);
		credits.push({
			artistId: outcome.profile.id,
			canonicalName: outcome.profile.name,
			creditedAs: proposal.creditedAs,
			role: proposal.role,
		});
	}
	return { credits, profiles: [...profiles.values()] };
}

async function applyCachedResolution(
	env: ArtistEnrichmentQueueEnv,
	main: MainArtistEnrichmentRpc,
	body: ArtistEnrichmentMessage,
	resolution: ArtistBillingResolution,
): Promise<void> {
	if (resolution.status !== "resolved") return;
	const artistIds = resolution.credits.map((credit) => credit.artistId);
	const profiles = await main.getCanonicalArtistProfiles(artistIds);
	if (profiles.length !== new Set(artistIds).size) {
		throw new ArtistResolutionProviderError(
			"cached artist resolution is missing canonical profiles",
			"deepseek",
			false,
		);
	}
	await publishResolution(env, main, body.festivalId, resolution, profiles);
}

async function publishResolution(
	env: ArtistEnrichmentQueueEnv,
	main: MainArtistEnrichmentRpc,
	festivalId: string,
	resolution: ArtistBillingResolution,
	profiles: ArtistProfile[],
): Promise<void> {
	const applied = await main.applyArtistBillingResolution(festivalId, resolution, profiles);
	if (!applied) return;
	const festivalIdObject = env.FESTIVAL_DO.idFromName(festivalId);
	const festival = env.FESTIVAL_DO.get(festivalIdObject) as unknown as FestivalArtistResolutionRpc;
	const status = await festival.applyArtistResolution(
		festivalId,
		applied.resolution,
		applied.profiles,
		applied.setIds,
	);
	if (status !== "stale") {
		await main.markArtistResolutionApplied(
			festivalId,
			applied.resolution.billingKey,
			applied.resolution.id,
			applied.resolution.version,
		);
	}
}

async function directArtistResolution(
	body: ArtistEnrichmentMessage,
	profile: ArtistProfile,
): Promise<ArtistBillingResolution> {
	const parsed = parseArtistBilling(body.billing, body.mbid);
	const input = resolutionInput(body);
	const inputHash = await sha256CanonicalJson({
		...artistResolutionCacheMaterial(input),
		stage: "deterministic-enrichment",
		profileId: profile.id,
	});
	return {
		id: `artist-resolution-v1-${inputHash.slice(0, 24)}`,
		sourceBilling: body.billing,
		billingKey: body.billingKey,
		status: "resolved",
		method: "deterministic",
		confidence: 1,
		credits: [
			{
				artistId: profile.id,
				canonicalName: profile.name,
				creditedAs: parsed.identityHint,
				role: parsed.presentedTitle ? "presenter" : "performer",
			},
		],
		...(parsed.presentedTitle ? { presentedTitle: parsed.presentedTitle } : {}),
		performanceQualifiers: parsed.performanceQualifiers,
		evidence: profile.provenance.map((item) => ({
			url: item.sourceUrl,
			title: `${profile.name} — ${item.provider}`,
			claims: ["act_identity"],
			retrievedAt: item.retrievedAt,
		})),
		inputHash,
		processorVersion: ARTIST_RESOLVER_VERSION,
		version: 1,
	};
}

function protocolResolution(
	body: ArtistEnrichmentMessage,
	result: ArtistResolutionResult,
	inputHash: string,
): ArtistBillingResolution {
	const proposedCredits: ArtistCreditProposal[] = result.credits.map((credit) => ({
		canonicalName: credit.canonicalName,
		creditedAs: credit.creditedAs,
		role: credit.role,
		confidence: credit.confidence,
	}));
	const evidence: ArtistResolutionEvidence[] = result.evidence.map((item) => ({
		url: item.url,
		title: item.title,
		claims: evidenceClaims(result, item.id),
		retrievedAt: new Date().toISOString(),
	}));
	return {
		id: `artist-resolution-v1-${inputHash.slice(0, 24)}`,
		sourceBilling: body.billing,
		billingKey: body.billingKey,
		status: result.status,
		method: "ai",
		confidence: result.confidence,
		credits: [],
		...(proposedCredits.length > 0 ? { proposedCredits } : {}),
		...(result.presentedTitle ? { presentedTitle: result.presentedTitle } : {}),
		performanceQualifiers: result.performanceQualifiers,
		evidence,
		inputHash,
		processorVersion: result.resolverVersion,
		model: result.model,
		version: 1,
	};
}

function evidenceClaims(
	result: ArtistResolutionResult,
	evidenceId: string,
): ArtistResolutionEvidence["claims"] {
	const claims = new Set<ArtistResolutionEvidence["claims"][number]>(["act_identity"]);
	for (const credit of result.credits) {
		if (!credit.evidenceIds.includes(evidenceId)) continue;
		if (
			credit.canonicalName.toLocaleLowerCase("en") !== credit.creditedAs.toLocaleLowerCase("en")
		) {
			claims.add("alias");
		}
		if (result.presentedTitle) claims.add("event_billing");
	}
	return [...claims];
}

async function unresolvedArtistResolution(
	body: ArtistEnrichmentMessage,
	input: ArtistResolutionInput,
): Promise<ArtistBillingResolution> {
	const inputHash = await resolutionInputHash(input);
	return {
		id: `artist-resolution-v1-${inputHash.slice(0, 24)}`,
		sourceBilling: body.billing,
		billingKey: body.billingKey,
		status: "unresolved",
		method: "ai",
		confidence: 0,
		credits: [],
		...(input.parsedBilling.presentedTitle
			? { presentedTitle: input.parsedBilling.presentedTitle }
			: {}),
		performanceQualifiers: [...input.parsedBilling.performanceQualifiers],
		evidence: [],
		inputHash,
		processorVersion: ARTIST_RESOLVER_VERSION,
		model: ARTIST_RESOLUTION_MODEL,
		version: 1,
	};
}

function resolutionInput(body: ArtistEnrichmentMessage): ArtistResolutionInput {
	const parsed = parseArtistBilling(body.billing, body.mbid);
	return {
		sourceBilling: body.billing,
		...(body.mbid ? { sourceMbid: body.mbid } : {}),
		contextBillings: body.contextBillings,
		parsedBilling: {
			coreBilling: parsed.coreBilling,
			identityHint: parsed.identityHint,
			performanceQualifiers: parsed.performanceQualifiers,
			...(parsed.presentedTitle ? { presentedTitle: parsed.presentedTitle } : {}),
		},
	};
}

async function resolutionInputHash(input: ArtistResolutionInput): Promise<string> {
	return sha256CanonicalJson({
		...artistResolutionCacheMaterial(input),
		stage: "billing-resolution",
	});
}

function artistResolutionConfigured(env: ArtistEnrichmentQueueEnv): boolean {
	return (
		env.DISABLE_ARTIST_RESOLUTION !== "true" &&
		env.ARTIST_RESOLUTION_MODEL !== undefined &&
		env.ARTIST_RESOLUTION_MODEL === ARTIST_RESOLUTION_MODEL &&
		Boolean(
			env.AI_GATEWAY_BASE_URL && env.AI_GATEWAY_TOKEN && env.DEEPSEEK_API_KEY && env.TAVILY_API_KEY,
		)
	);
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
		(candidate.billingKey !== undefined &&
			(typeof candidate.billingKey !== "string" || candidate.billingKey.length > 500)) ||
		typeof candidate.billing !== "string" ||
		candidate.billing.length > 300 ||
		(candidate.mbid !== undefined &&
			(typeof candidate.mbid !== "string" || candidate.mbid.length > 100)) ||
		!Array.isArray(candidate.setIds) ||
		candidate.setIds.length > 500 ||
		!candidate.setIds.every((setId) => typeof setId === "string" && setId.length <= 200) ||
		(candidate.contextBillings !== undefined &&
			(!Array.isArray(candidate.contextBillings) ||
				candidate.contextBillings.length > 250 ||
				!candidate.contextBillings.every(
					(billing) => typeof billing === "string" && billing.length <= 300,
				)))
	) {
		return null;
	}
	const mbid = typeof candidate.mbid === "string" ? candidate.mbid : undefined;
	const billingKey =
		typeof candidate.billingKey === "string"
			? candidate.billingKey
			: parseArtistBilling(candidate.billing, mbid).billingKey;
	const contextBillings = Array.isArray(candidate.contextBillings)
		? (candidate.contextBillings as string[])
		: [candidate.billing];
	return {
		jobId: candidate.jobId,
		sourceKey: candidate.sourceKey,
		festivalId: candidate.festivalId,
		billing: candidate.billing,
		billingKey,
		contextBillings,
		setIds: candidate.setIds as string[],
		...(mbid ? { mbid } : {}),
	};
}

function wait(milliseconds: number): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, milliseconds));
}
