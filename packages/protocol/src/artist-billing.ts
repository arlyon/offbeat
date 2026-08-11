import type { PerformanceQualifier } from "./types.js";

const MBID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

const TRAILING_QUALIFIERS: ReadonlyArray<{
	pattern: RegExp;
	qualifier: PerformanceQualifier;
}> = [
	{ pattern: /\s*\(\s*dj\s+set\s*\)\s*$/i, qualifier: "dj_set" },
	{ pattern: /\s+dj\s+set\s*$/i, qualifier: "dj_set" },
	{ pattern: /\s*\(\s*live\s*\)\s*$/i, qualifier: "live" },
	{ pattern: /\s*\(\s*ambient\s+set\s*\)\s*$/i, qualifier: "ambient_set" },
	{ pattern: /\s*\(\s*hybride?\s+set\s*\)\s*$/i, qualifier: "hybrid_set" },
];

export interface ParsedArtistBilling {
	/** Exact input string. This is the display value and must never be rewritten. */
	sourceBilling: string;
	/** NFKC/trimmed billing with only safe trailing performance qualifiers removed. */
	coreBilling: string;
	/** Conservative identity hint. It is never split on punctuation. */
	identityHint: string;
	/** Title following a syntactically explicit `present(s)` marker. */
	presentedTitle?: string;
	performanceQualifiers: PerformanceQualifier[];
	billingKey: string;
}

/** Normalize a billing only for comparison. The source string remains authoritative. */
export function normalizeArtistBilling(value: string): string {
	return value
		.normalize("NFKC")
		.trim()
		.toLocaleLowerCase("en")
		.replace(/[‘’]/g, "'")
		.replace(/\s+(?:and|\+)\s+/g, " & ")
		.replace(/\s+/g, " ");
}

/** Parse safe structural hints without guessing how many artists are credited. */
export function parseArtistBilling(value: string, sourceMbid?: string): ParsedArtistBilling {
	let coreBilling = value.normalize("NFKC").trim();
	const performanceQualifiers: PerformanceQualifier[] = [];

	let changed = true;
	while (changed && coreBilling) {
		changed = false;
		for (const { pattern, qualifier } of TRAILING_QUALIFIERS) {
			const stripped = coreBilling.replace(pattern, "").trim();
			if (stripped === coreBilling || !stripped) continue;
			coreBilling = stripped;
			if (!performanceQualifiers.includes(qualifier)) {
				performanceQualifiers.unshift(qualifier);
			}
			changed = true;
			break;
		}
	}

	const presentation = coreBilling.match(/^(.+?)\s+present(?:s|ing)?\s+(.+)$/i);
	const identityHint = presentation?.[1]?.trim() || coreBilling;
	const presentedTitle = presentation?.[2]?.trim();
	const normalizedMbid = sourceMbid?.trim().toLowerCase();
	const normalizedBilling = normalizeArtistBilling(value);
	const billingKey =
		normalizedMbid && MBID_PATTERN.test(normalizedMbid)
			? `name:${normalizedBilling}|mbid:${normalizedMbid}`
			: `name:${normalizedBilling}`;

	return {
		sourceBilling: value,
		coreBilling,
		identityHint,
		...(presentedTitle ? { presentedTitle } : {}),
		performanceQualifiers,
		billingKey,
	};
}
