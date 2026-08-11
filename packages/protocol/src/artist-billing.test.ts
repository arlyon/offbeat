import { describe, expect, it } from "vitest";
import houghtonCorpusJson from "../fixtures/artist-billings/houghton-2026.json";
import lostVillageResponseJson from "../fixtures/clashfinder/lost-village-2026.json";
import type { ClashfinderApiResponse } from "./clashfinder-api.js";
import { normalizeArtistBilling, parseArtistBilling } from "./artist-billing.js";
import { parseClashfinderApi } from "./clashfinder.js";

interface BillingFixture {
	festival: string;
	billings: Array<{
		sourceBilling: string;
		expectedCanonicalNames: string[];
		expectedPresentedTitle?: string;
		expectedQualifiers?: string[];
	}>;
}

describe("artist billing parser", () => {
	it.each([
		["Midland (DJ SET)", "Midland", ["dj_set"]],
		["Chibi Ichigo DJ Set", "Chibi Ichigo", ["dj_set"]],
		["Marie Davidson (LIVE)", "Marie Davidson", ["live"]],
		["Midland (AMBIENT SET)", "Midland", ["ambient_set"]],
		["Spinvis (Hybride Set)", "Spinvis", ["hybrid_set"]],
	] as const)("extracts a safe trailing qualifier from %s", (source, core, qualifiers) => {
		const parsed = parseArtistBilling(source);
		expect(parsed.sourceBilling).toBe(source);
		expect(parsed.coreBilling).toBe(core);
		expect(parsed.performanceQualifiers).toEqual(qualifiers);
	});

	it("extracts a presented title without splitting its presenters", () => {
		const parsed = parseArtistBilling("Harry & Dan Present Tea Dance");
		expect(parsed).toMatchObject({
			sourceBilling: "Harry & Dan Present Tea Dance",
			coreBilling: "Harry & Dan Present Tea Dance",
			identityHint: "Harry & Dan",
			presentedTitle: "Tea Dance",
		});
	});

	it.each([
		"Chase & Status",
		"Earth, Wind & Fire",
		"Hamish & Toby",
		"COAST 2 COAST (THE GHOST & GENE ON EARTH)",
		"BV/XT (Ben Vince + Xterea)",
	])("never infers artist boundaries from punctuation in %s", (source) => {
		const parsed = parseArtistBilling(source);
		expect(parsed.identityHint).toBe(source);
	});

	it("normalizes separators only for matching and keeps the source exact", () => {
		const source = "  Dan  and  Harry Present Tea Dance  ";
		const parsed = parseArtistBilling(source);
		expect(parsed.sourceBilling).toBe(source);
		expect(normalizeArtistBilling(parsed.coreBilling)).toBe("dan & harry present tea dance");
	});

	it("uses the exact normalized source plus a valid MBID as the billing key", () => {
		const mbid = "a74b1b7f-71a5-4011-9441-d0b5e4122711";
		expect(parseArtistBilling("Misspelled Artist", mbid).billingKey).toBe(
			`name:misspelled artist|mbid:${mbid}`,
		);
		expect(parseArtistBilling("Midland (DJ SET)", mbid).billingKey).toBe(
			`name:midland (dj set)|mbid:${mbid}`,
		);
		expect(parseArtistBilling("Midland (LIVE)", mbid).billingKey).not.toBe(
			parseArtistBilling("Midland (DJ SET)", mbid).billingKey,
		);
	});
});

describe("2026 festival billing fixtures", () => {
	it("preserves every Lost Village source billing exactly", () => {
		const response = lostVillageResponseJson as ClashfinderApiResponse;
		const sourceBillings = response.locations.flatMap((location) =>
			location.events.map((event) => event.name),
		);
		const lineup = parseClashfinderApi("lost-village-2026", response);

		expect(lineup.sets).toHaveLength(28);
		expect(lineup.sets.map((set) => set.artist)).toEqual(sourceBillings);
		expect(lineup.sets.map((set) => set.sourceBilling)).toEqual(sourceBillings);
		expect(lineup.sets.find((set) => set.artist === "KAVARI (dj set)")).toMatchObject({
			artist: "KAVARI (dj set)",
			sourceBilling: "KAVARI (dj set)",
			performanceQualifiers: ["dj_set"],
		});
	});

	it("covers the reviewed Houghton billings without hard-coded splitting", () => {
		const corpus = houghtonCorpusJson as BillingFixture;
		expect(corpus.festival).toBe("Houghton Festival 2026");

		for (const billing of corpus.billings) {
			const parsed = parseArtistBilling(billing.sourceBilling);
			expect(parsed.sourceBilling).toBe(billing.sourceBilling);
			if (billing.expectedQualifiers) {
				expect(parsed.performanceQualifiers).toEqual(billing.expectedQualifiers);
			}
		}

		expect(parseArtistBilling("Harry & Dan Present Tea Dance").presentedTitle).toBe(
			"Tea Dance",
		);
		expect(
			parseArtistBilling("COAST 2 COAST (THE GHOST & GENE ON EARTH)").identityHint,
		).toBe("COAST 2 COAST (THE GHOST & GENE ON EARTH)");
	});
});
