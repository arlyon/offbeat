import type {
	ArtistBillingResolution,
	ArtistCredit,
	ArtistCreditProposal,
	ArtistProfile,
	ArtistResolutionEvidence,
} from "@offbeat/protocol";
import { normalizeArtistBilling, parseArtistBilling } from "@offbeat/protocol";
import {
	type ArtistEnrichmentMessage,
	type ArtistEnrichmentOutcome,
	ArtistProviderError,
	artistEnrichmentSourceKey,
	enrichArtist,
	isAmbiguousArtistBilling,
} from "./artist-enrichment";
import {
	ARTIST_RESOLUTION_MODEL,
	ARTIST_RESOLVER_VERSION,
	type ArtistResolutionInput,
	ArtistResolutionProviderError,
	type ArtistResolutionResult,
	artistIdentitySearchCacheKeys,
	artistResolutionCacheMaterial,
	canonicalResidentAdvisorProfileUrl,
	createArtistResolutionSearchCacheKey,
	discoverMusicBrainzArtistId,
	discoverResidentAdvisorProfile,
	discoverResidentAdvisorProfileUrl,
	resolveArtistBilling,
	searchArtistIdentityBatch,
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
	BRAVE_SEARCH_API_KEY?: string;
	TAVILY_API_KEY?: string;
	DISABLE_ARTIST_RESOLUTION?: string;
}

const MAX_ARTIST_PROFILE_LINKS = 20;

function exactLocalArtistMatches(profiles: ArtistProfile[], name: string): ArtistProfile[] {
	const normalizedName = normalizeArtistBilling(name);
	return profiles.filter((profile) =>
		[profile.name, ...profile.aliases].some(
			(candidate) => normalizeArtistBilling(candidate) === normalizedName,
		),
	);
}

function needsCanonicalEnrichment(profile: ArtistProfile): boolean {
	return (
		Boolean(profile.mbid) &&
		profile.provenance.some((item) => item.provider === "admin") &&
		!profile.provenance.some(
			(item) => item.provider === "musicbrainz" || item.provider === "wikidata",
		)
	);
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
	getCanonicalArtistProfilesByName(names: string[]): Promise<ArtistProfile[]>;
	searchCanonicalArtistProfiles(query: string, limit?: number): Promise<ArtistProfile[]>;
	getCachedArtistResolutionSearch(cacheKey: string): Promise<unknown | null>;
	putCachedArtistResolutionSearch(
		cacheKey: string,
		response: unknown,
		provider?: "brave" | "exhausted" | "tavily",
	): Promise<void>;
	deleteCachedArtistResolutionSearches(cacheKeys: string[]): Promise<void>;
	getArtistSearchProviderAttempts(identityKeys: string[]): Promise<Record<string, number>>;
	recordArtistSearchProviderAttempts(
		identityKeys: string[],
		provider: "brave" | "tavily",
	): Promise<void>;
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

interface ArtistSearchPrefetchPlan {
	transientKeys: string[];
	exhaustedKeys: string[];
	retryBillingKeys: Set<string>;
}

interface ArtistSearchPrefetchEntry {
	body: ArtistEnrichmentMessage;
	name: string;
	identityKey: string;
	cacheKeys: string[];
}

async function prefetchArtistSearchBatch(
	messages: readonly Message<unknown>[],
	main: MainArtistEnrichmentRpc,
	limiter: ArtistEnrichmentLimiterRpc,
	env: ArtistEnrichmentQueueEnv,
): Promise<ArtistSearchPrefetchPlan> {
	const plan: ArtistSearchPrefetchPlan = {
		transientKeys: [],
		exhaustedKeys: [],
		retryBillingKeys: new Set(),
	};
	if (
		env.DISABLE_ARTIST_RESOLUTION === "true" ||
		(!env.BRAVE_SEARCH_API_KEY && !env.TAVILY_API_KEY)
	) {
		return plan;
	}
	const candidateBodies = messages
		.map((message) => parseArtistEnrichmentMessage(message.body))
		.filter((body): body is ArtistEnrichmentMessage => Boolean(body));
	const bodies: ArtistEnrichmentMessage[] = [];
	for (const body of candidateBodies) {
		const cachedResolution = await main.getCachedArtistBillingResolution(body.billingKey);
		if (
			cachedResolution?.status === "resolved" &&
			(cachedResolution.method === "manual" ||
				cachedResolution.processorVersion === ARTIST_RESOLVER_VERSION)
		) {
			continue;
		}
		const parsed = parseArtistBilling(body.billing, body.mbid);
		const profiles = await main.getCanonicalArtistProfilesByName([parsed.identityHint]);
		const cachedEnrichment = await main.getCachedArtistEnrichment(
			artistEnrichmentSourceKey(parsed.identityHint, body.mbid),
		);
		const profile =
			profiles.length === 1
				? profiles[0]
				: cachedEnrichment?.status === "enriched"
					? cachedEnrichment.profile
					: undefined;
		if (profile?.links.some((link) => Boolean(canonicalResidentAdvisorProfileUrl(link.url)))) {
			continue;
		}
		bodies.push(body);
		if (bodies.length === 5) break;
	}

	const entries: ArtistSearchPrefetchEntry[] = [];
	for (const body of bodies) {
		const input = resolutionInput(body);
		const identityCacheKeys = await artistIdentitySearchCacheKeys(input.parsedBilling.identityHint);
		const identityKey = identityCacheKeys[0];
		if (!identityKey) continue;
		entries.push({
			body,
			name: input.parsedBilling.identityHint,
			identityKey,
			cacheKeys: [await createArtistResolutionSearchCacheKey(input), ...identityCacheKeys],
		});
	}
	if (entries.length === 0) return plan;

	const attempts = await main.getArtistSearchProviderAttempts(
		entries.map((entry) => entry.identityKey),
	);
	const groups: Record<"brave" | "tavily", ArtistSearchPrefetchEntry[]> = {
		brave: [],
		tavily: [],
	};
	const freshProvider = await limiter.nextArtistSearchProvider(
		Boolean(env.BRAVE_SEARCH_API_KEY),
		Boolean(env.TAVILY_API_KEY),
	);
	for (const entry of entries) {
		const attemptMask = attempts[entry.identityKey] ?? 0;
		if (attemptMask === 3) {
			for (const cacheKey of entry.cacheKeys) {
				await main.putCachedArtistResolutionSearch(cacheKey, [], "exhausted");
			}
			continue;
		}
		const provider = attemptMask === 1 ? "tavily" : attemptMask === 2 ? "brave" : freshProvider;
		if (
			(provider === "brave" && !env.BRAVE_SEARCH_API_KEY) ||
			(provider === "tavily" && !env.TAVILY_API_KEY)
		) {
			for (const cacheKey of entry.cacheKeys) {
				await main.putCachedArtistResolutionSearch(cacheKey, [], "exhausted");
			}
			continue;
		}
		groups[provider].push(entry);
	}

	for (const provider of ["brave", "tavily"] as const) {
		const group = groups[provider];
		if (group.length === 0) continue;
		const response = await searchArtistIdentityBatch(
			group.map((entry) => entry.name),
			provider,
			{
				braveApiKey: env.BRAVE_SEARCH_API_KEY,
				tavilyApiKey: env.TAVILY_API_KEY,
			},
		);
		const cacheKeys = [...new Set(group.flatMap((entry) => entry.cacheKeys))];
		await Promise.all(
			cacheKeys.map((cacheKey) =>
				main.putCachedArtistResolutionSearch(cacheKey, [response], provider),
			),
		);
		await main.recordArtistSearchProviderAttempts(
			group.map((entry) => entry.identityKey),
			provider,
		);
		plan.transientKeys.push(...cacheKeys);
		const providerMask = provider === "brave" ? 1 : 2;
		for (const entry of group) {
			const nextMask = (attempts[entry.identityKey] ?? 0) | providerMask;
			if (nextMask === 3) plan.exhaustedKeys.push(...entry.cacheKeys);
			else plan.retryBillingKeys.add(entry.body.billingKey);
		}
	}
	return plan;
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
	nextArtistSearchProvider(
		braveAvailable: boolean,
		tavilyAvailable: boolean,
	): Promise<"brave" | "tavily">;
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

	const searchPlan = await prefetchArtistSearchBatch(batch.messages, main, limiter, env);

	for (const message of batch.messages) {
		const body = parseArtistEnrichmentMessage(message.body);
		if (!body) {
			message.ack();
			continue;
		}
		try {
			const cachedResolution = await main.getCachedArtistBillingResolution(body.billingKey);
			if (
				cachedResolution?.status === "resolved" &&
				(cachedResolution.method === "manual" ||
					cachedResolution.processorVersion === ARTIST_RESOLVER_VERSION)
			) {
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

			const parsedBilling = parseArtistBilling(body.billing, body.mbid);
			const existingProfiles = exactLocalArtistMatches(
				await main.searchCanonicalArtistProfiles(parsedBilling.identityHint),
				parsedBilling.identityHint,
			);
			if (existingProfiles.length === 1) {
				let existingProfile = existingProfiles[0];
				if (existingProfile && needsCanonicalEnrichment(existingProfile)) {
					const enrichment = await enrichCandidate(
						main,
						limiter,
						env,
						{
							...body,
							billing: existingProfile.name,
							mbid: existingProfile.mbid,
							sourceKey: artistEnrichmentSourceKey(existingProfile.name, existingProfile.mbid),
						},
						true,
					);
					if (enrichment.status === "enriched") existingProfile = enrichment.profile;
				}
				const hasProviderBoundRaLink = existingProfile?.links.some((link) =>
					Boolean(canonicalResidentAdvisorProfileUrl(link.url)),
				);
				if (
					existingProfile &&
					(!isCollisionProneBilling(parsedBilling.identityHint) || hasProviderBoundRaLink)
				) {
					const profile = await attachResidentAdvisorLink(main, env, existingProfile);
					const resolution = await directArtistResolution(body, profile);
					await publishResolution(env, main, body.festivalId, resolution, [profile]);
					await main.markArtistResolutionComplete(body.jobId, "resolved");
					message.ack();
					continue;
				}
			}
			const directCandidate = {
				...body,
				billing: parsedBilling.identityHint,
				sourceKey: artistEnrichmentSourceKey(parsedBilling.identityHint, body.mbid),
			};
			const directOutcome = await enrichCandidate(main, limiter, env, directCandidate, false);
			if (directOutcome.status === "enriched") {
				const hasProviderBoundRaLink = directOutcome.profile.links.some((link) =>
					Boolean(canonicalResidentAdvisorProfileUrl(link.url)),
				);
				if (
					body.mbid ||
					!isCollisionProneBilling(parsedBilling.identityHint) ||
					hasProviderBoundRaLink
				) {
					const profile = await attachResidentAdvisorLink(main, env, directOutcome.profile);
					const resolution = await directArtistResolution(body, profile);
					await publishResolution(env, main, body.festivalId, resolution, [profile]);
					await main.markArtistResolutionComplete(body.jobId, "resolved");
					message.ack();
					continue;
				}
			}

			const discoveredProfile = isAmbiguousArtistBilling(parsedBilling.identityHint)
				? null
				: await discoverTrustedArtistProfile(
						main,
						limiter,
						env,
						body,
						parsedBilling.identityHint,
						isDistinctiveBilling(parsedBilling.identityHint),
					);
			if (discoveredProfile) {
				const resolution = await directArtistResolution(body, discoveredProfile);
				await publishResolution(env, main, body.festivalId, resolution, [discoveredProfile]);
				await main.markArtistResolutionComplete(body.jobId, "resolved");
				message.ack();
				continue;
			}

			const contextual = await resolveContextualCredits(main, limiter, env, body, parsedBilling);
			if (contextual) {
				await publishResolution(
					env,
					main,
					body.festivalId,
					contextual.resolution,
					contextual.profiles,
				);
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
				timeoutMs: 60_000,
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
				if (searchPlan.retryBillingKeys.has(body.billingKey)) {
					message.retry({ delaySeconds: 15 });
				} else {
					message.ack();
				}
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
				if (searchPlan.retryBillingKeys.has(body.billingKey)) {
					message.retry({ delaySeconds: 15 });
				} else {
					message.ack();
				}
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
				console.error(`[artist-enrichment] provider rejected job ${body.jobId}: ${detail}`);
				const input = resolutionInput(body);
				const unresolved = await unresolvedArtistResolution(body, input);
				await main.recordArtistBillingResolution(unresolved);
				await main.markArtistResolutionComplete(body.jobId, "unresolved");
				message.ack();
				continue;
			}
			await main.markArtistEnrichmentFailure(body.jobId, detail);
			const delaySeconds = Math.min(900, 15 * 2 ** Math.min(message.attempts, 6));
			console.error(
				`[artist-enrichment] retrying job ${body.jobId} in ${delaySeconds}s: ${detail}`,
			);
			message.retry({ delaySeconds });
		}
	}
	if (searchPlan.transientKeys.length > 0) {
		await main.deleteCachedArtistResolutionSearches(searchPlan.transientKeys);
	}
	for (const cacheKey of [...new Set(searchPlan.exhaustedKeys)]) {
		await main.putCachedArtistResolutionSearch(cacheKey, [], "exhausted");
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
		try {
			outcome = await enrichArtist(candidate, {
				userAgent: env.MUSICBRAINZ_USER_AGENT,
				allowAmbiguousBilling,
				beforeMusicBrainzRequest: async () => {
					const delayMs = await limiter.reserveMusicBrainz();
					if (delayMs > 0) await wait(delayMs);
				},
			});
		} catch (error) {
			if (!candidate.mbid && error instanceof ArtistProviderError) {
				console.warn("[artist-enrichment] MusicBrainz name lookup unavailable; using web evidence");
				return { status: "unresolved", reason: "musicbrainz_unavailable" };
			}
			throw error;
		}
		await main.cacheCanonicalArtistEnrichment(sourceKey, outcome);
	}
	return outcome;
}

async function discoverTrustedArtistProfile(
	main: MainArtistEnrichmentRpc,
	limiter: ArtistEnrichmentLimiterRpc,
	env: ArtistEnrichmentQueueEnv,
	body: ArtistEnrichmentMessage,
	artistName: string,
	allowCollisionProne = false,
): Promise<ArtistProfile | null> {
	if (!env.BRAVE_SEARCH_API_KEY?.trim() && !env.TAVILY_API_KEY?.trim()) return null;
	const discoveryOptions = {
		tavilyApiKey: env.TAVILY_API_KEY ?? "",
		cache: {
			getSearch: (cacheKey: string) => main.getCachedArtistResolutionSearch(cacheKey),
			putSearch: (cacheKey: string, response: unknown) =>
				main.putCachedArtistResolutionSearch(cacheKey, response),
		},
	};
	try {
		const [mbidResult, raResult] = await Promise.allSettled([
			discoverMusicBrainzArtistId(artistName, discoveryOptions),
			discoverResidentAdvisorProfile(artistName, discoveryOptions),
		]);
		const mbid = mbidResult.status === "fulfilled" ? mbidResult.value : undefined;
		const raProfile = raResult.status === "fulfilled" ? raResult.value : undefined;
		if (mbid) {
			const candidate: ArtistEnrichmentMessage = {
				...body,
				billing: artistName,
				mbid,
				sourceKey: artistEnrichmentSourceKey(artistName, mbid),
			};
			const outcome = await enrichCandidate(main, limiter, env, candidate, true);
			if (outcome.status === "enriched") {
				const providerRaUrl = outcome.profile.links
					.map((link) => canonicalResidentAdvisorProfileUrl(link.url))
					.find((url) => url !== undefined);
				const collisionSafe =
					!isCollisionProneBilling(artistName) ||
					Boolean(raProfile && providerRaUrl === raProfile.url);
				if (collisionSafe) {
					const profile = raProfile
						? withResidentAdvisorLink(outcome.profile, raProfile.url)
						: outcome.profile;
					await main.cacheCanonicalArtistEnrichment(candidate.sourceKey, {
						status: "enriched",
						profile,
					});
					return profile;
				}
			}
		}
		if (!raProfile || (isCollisionProneBilling(artistName) && !allowCollisionProne)) return null;
		const retrievedAt = new Date().toISOString();
		const aliases =
			raProfile.name.normalize("NFKC").toLocaleLowerCase("en") ===
			artistName.normalize("NFKC").toLocaleLowerCase("en")
				? []
				: [artistName];
		const profile: ArtistProfile = {
			id: `ra:${raProfile.slug.toLowerCase()}`,
			name: raProfile.name,
			aliases,
			genres: [],
			links: [{ kind: "resident_advisor", url: raProfile.url }],
			provenance: [
				{
					field: "identity,links",
					provider: "resident_advisor",
					sourceUrl: raProfile.url,
					license: "outbound-link-only",
					retrievedAt,
				},
			],
			updatedAt: retrievedAt,
		};
		await main.cacheCanonicalArtistEnrichment(artistEnrichmentSourceKey(artistName), {
			status: "enriched",
			profile,
		});
		return profile;
	} catch (error) {
		const detail = error instanceof Error ? error.message : String(error);
		console.warn(`[artist-enrichment] trusted identity discovery failed: ${detail}`);
		return null;
	}
}

function isCollisionProneBilling(value: string): boolean {
	const tokens = value
		.normalize("NFKC")
		.toLocaleLowerCase("en")
		.replace(/[^\p{L}\p{N}]+/gu, " ")
		.trim()
		.split(/\s+/)
		.filter(Boolean);
	return tokens.length <= 1;
}

async function resolveContextualCredits(
	main: MainArtistEnrichmentRpc,
	limiter: ArtistEnrichmentLimiterRpc,
	env: ArtistEnrichmentQueueEnv,
	body: ArtistEnrichmentMessage,
	parsed: ReturnType<typeof parseArtistBilling>,
): Promise<{ resolution: ArtistBillingResolution; profiles: ArtistProfile[] } | null> {
	const spans = explicitCreditSpans(parsed.identityHint);
	if (spans.length < 2) return null;
	const profiles: ArtistProfile[] = [];
	for (const span of spans) {
		const existing = exactLocalArtistMatches(await main.searchCanonicalArtistProfiles(span), span);
		let profile = existing.length === 1 ? existing[0] : undefined;
		if (
			profile &&
			isCollisionProneBilling(span) &&
			!profile.links.some((link) => canonicalResidentAdvisorProfileUrl(link.url))
		) {
			profile = undefined;
		}
		profile ??=
			(await discoverTrustedArtistProfile(
				main,
				limiter,
				env,
				body,
				span,
				isDistinctiveBilling(span),
			)) ?? undefined;
		if (!profile) return null;
		profiles.push(profile);
	}
	const input = resolutionInput(body);
	const inputHash = await sha256CanonicalJson({
		...artistResolutionCacheMaterial(input),
		stage: "contextual-components",
		profileIds: profiles.map((profile) => profile.id),
	});
	const featuredIndex = /\b(?:feat(?:uring)?|ft)\.?\b/i.test(parsed.identityHint) ? 1 : -1;
	return {
		resolution: {
			id: `artist-resolution-v1-${inputHash.slice(0, 24)}`,
			sourceBilling: body.billing,
			billingKey: body.billingKey,
			status: "resolved",
			method: "deterministic",
			confidence: 1,
			credits: profiles.map((profile, index) => ({
				artistId: profile.id,
				canonicalName: profile.name,
				creditedAs: spans[index] ?? profile.name,
				role: parsed.presentedTitle
					? "presenter"
					: featuredIndex >= 0 && index >= featuredIndex
						? "guest"
						: "performer",
			})),
			...(parsed.presentedTitle ? { presentedTitle: parsed.presentedTitle } : {}),
			performanceQualifiers: parsed.performanceQualifiers,
			evidence: profiles.flatMap((profile) =>
				profile.provenance.map((item) => ({
					url: item.sourceUrl,
					title: `${profile.name} — ${item.provider}`,
					claims: ["act_identity" as const],
					retrievedAt: item.retrievedAt,
				})),
			),
			inputHash,
			processorVersion: ARTIST_RESOLVER_VERSION,
			version: 1,
		},
		profiles,
	};
}

function explicitCreditSpans(identity: string): string[] {
	const parenthetical = identity.match(
		/\(([^()]*(?:&|\band\b|\bfeat(?:uring)?\b|\bwith\b)[^()]*)\)\s*$/i,
	);
	const candidate = parenthetical?.[1]?.trim() || identity;
	const spans = candidate
		.split(
			/\s*(?:&|,|\band\b|\bb2b\b|\bversus\b|\bvs\.?\b|\bfeat(?:uring)?\.?\b|\bft\.?\b|\bwith\b|\s+x\s+)\s*/i,
		)
		.map((span) => span.trim())
		.filter(Boolean);
	return spans.length >= 2 && spans.length <= 8 ? spans : [];
}

function isDistinctiveBilling(value: string): boolean {
	return /[^\p{L}\p{N}\s]/u.test(value);
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
		const existingProfiles = exactLocalArtistMatches(
			await main.searchCanonicalArtistProfiles(proposal.canonicalName),
			proposal.canonicalName,
		);
		const existingProfile = existingProfiles.length === 1 ? existingProfiles[0] : undefined;
		const candidate: ArtistEnrichmentMessage = {
			...body,
			billing: proposal.canonicalName,
			sourceKey: artistEnrichmentSourceKey(proposal.canonicalName),
		};
		const outcome = existingProfile
			? ({ status: "enriched", profile: existingProfile } as const)
			: await enrichCandidate(main, limiter, env, candidate, true);
		const profile =
			outcome.status === "enriched"
				? await attachResidentAdvisorLink(main, env, outcome.profile)
				: await discoverTrustedArtistProfile(
						main,
						limiter,
						env,
						body,
						proposal.canonicalName,
						true,
					);
		if (!profile) return null;
		profiles.set(profile.id, profile);
		credits.push({
			artistId: profile.id,
			canonicalName: profile.name,
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
	const enrichedProfiles = await Promise.all(
		profiles.map((profile) => attachResidentAdvisorLink(main, env, profile)),
	);
	await publishResolution(env, main, body.festivalId, resolution, enrichedProfiles);
}

async function attachResidentAdvisorLink(
	main: MainArtistEnrichmentRpc,
	env: ArtistEnrichmentQueueEnv,
	profile: ArtistProfile,
): Promise<ArtistProfile> {
	const existingUrl = profile.links
		.map((link) => canonicalResidentAdvisorProfileUrl(link.url))
		.find((url) => url !== undefined);
	if (existingUrl) {
		if (
			profile.links.some((link) => link.kind === "resident_advisor" && link.url === existingUrl)
		) {
			return profile;
		}
		return withResidentAdvisorLink(profile, existingUrl);
	}
	if (profile.id.startsWith("admin:")) return profile;
	if (!env.BRAVE_SEARCH_API_KEY?.trim() && !env.TAVILY_API_KEY?.trim()) return profile;
	try {
		const url = await discoverResidentAdvisorProfileUrl(profile.name, {
			tavilyApiKey: env.TAVILY_API_KEY ?? "",
			cache: {
				getSearch: (cacheKey) => main.getCachedArtistResolutionSearch(cacheKey),
				putSearch: (cacheKey, response) => main.putCachedArtistResolutionSearch(cacheKey, response),
			},
		});
		return url ? withResidentAdvisorLink(profile, url) : profile;
	} catch (error) {
		const detail = error instanceof Error ? error.message : String(error);
		console.warn(`[artist-enrichment] optional RA link lookup failed: ${detail}`);
		return profile;
	}
}

function withResidentAdvisorLink(profile: ArtistProfile, url: string): ArtistProfile {
	const links = [
		{ kind: "resident_advisor" as const, url },
		...profile.links.filter((link) => canonicalResidentAdvisorProfileUrl(link.url) !== url),
	]
		.sort((left, right) => left.kind.localeCompare(right.kind) || left.url.localeCompare(right.url))
		.slice(0, MAX_ARTIST_PROFILE_LINKS);
	return {
		...profile,
		links,
		updatedAt: new Date().toISOString(),
	};
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
			env.AI_GATEWAY_BASE_URL &&
				env.AI_GATEWAY_TOKEN &&
				env.DEEPSEEK_API_KEY &&
				(env.BRAVE_SEARCH_API_KEY || env.TAVILY_API_KEY),
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
