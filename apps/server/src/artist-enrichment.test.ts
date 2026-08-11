import { describe, expect, it, vi } from "vitest";
import {
	ArtistProviderError,
	enrichArtist,
	isAmbiguousArtistBilling,
} from "./artist-enrichment";

const MBID = "a74b1b7f-71a5-4011-9441-d0b5e4122711";
const NOW = new Date("2026-08-10T12:00:00.000Z");

function jsonResponse(value: unknown, status = 200): Response {
	return new Response(JSON.stringify(value), {
		status,
		headers: { "Content-Type": "application/json" },
	});
}

describe("artist enrichment", () => {
	it("skips collaboration billings until disambiguation", async () => {
		const fetcher = vi.fn<typeof fetch>();
		const outcome = await enrichArtist(
			{
				festivalId: "festival",
				setIds: ["set-1"],
				billing: "Dr. Banana & Josh T",
			},
			{ userAgent: "Offbeat/Test", fetch: fetcher },
		);

		expect(outcome).toEqual({ status: "unresolved", reason: "ambiguous_billing" });
		expect(fetcher).not.toHaveBeenCalled();
	});

	it("does not let a source MBID collapse a composite billing", async () => {
		const fetcher = vi.fn<typeof fetch>();
		const outcome = await enrichArtist(
			{
				festivalId: "festival",
				setIds: ["set-1"],
				billing: "Raisa K feat. Coby Sey",
				mbid: MBID,
			},
			{ userAgent: "Offbeat/Test", fetch: fetcher },
		);
		expect(outcome).toEqual({ status: "unresolved", reason: "ambiguous_billing" });
		expect(fetcher).not.toHaveBeenCalled();
	});

	it("rejects a source MBID whose canonical name and aliases do not match the billing", async () => {
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(
				jsonResponse({ id: MBID, name: "Different Artist", aliases: [{ name: "Other Name" }] }),
			);
		const outcome = await enrichArtist(
			{ festivalId: "festival", setIds: ["set-1"], billing: "Example Artist", mbid: MBID },
			{ userAgent: "Offbeat/Test", fetch: fetcher },
		);
		expect(outcome).toEqual({ status: "unresolved", reason: "mbid_name_mismatch" });
	});

	it("recognizes only explicit collaboration separators", () => {
		expect(isAmbiguousArtistBilling("A & B")).toBe(true);
		expect(isAmbiguousArtistBilling("A b2b B")).toBe(true);
		expect(isAmbiguousArtistBilling("A vs. B")).toBe(true);
		expect(isAmbiguousArtistBilling("Chase & Status")).toBe(true);
		expect(isAmbiguousArtistBilling("Earth, Wind & Fire")).toBe(true);
		expect(isAmbiguousArtistBilling("The xx")).toBe(false);
	});

	it("builds a bounded offline profile from MusicBrainz and Wikidata", async () => {
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(
				jsonResponse({
					id: MBID,
					name: "Example Artist",
					type: "Person",
					country: "GB",
					aliases: [{ name: "Example Artist" }, { name: "Example" }],
					tags: [
						{ name: "Electronic", count: 7 },
						{ name: "House", count: 12 },
					],
					relations: [
						{ type: "streaming music", url: { resource: "https://open.spotify.com/artist/abc" } },
						{ type: "soundcloud", url: { resource: "https://soundcloud.com/example" } },
						{ type: "social network", url: { resource: "https://ra.co/dj/exampleartist" } },
						{ type: "official homepage", url: { resource: "https://example.com/" } },
						{ type: "wikidata", url: { resource: "https://www.wikidata.org/wiki/Q42" } },
						{ type: "bad", url: { resource: "javascript:alert(1)" } },
					],
				}),
			)
			.mockResolvedValueOnce(
				jsonResponse({
					entities: { Q42: { descriptions: { en: { value: "British electronic musician" } } } },
				}),
			);

		const outcome = await enrichArtist(
			{ festivalId: "festival", setIds: ["set-1"], billing: "Example Artist", mbid: MBID },
			{ userAgent: "Offbeat/Test", fetch: fetcher, now: () => NOW },
		);

		expect(outcome).toMatchObject({
			status: "enriched",
			profile: {
				id: `mbid:${MBID}`,
				name: "Example Artist",
				mbid: MBID,
				wikidataId: "Q42",
				aliases: ["Example"],
				artistType: "Person",
				country: "GB",
				genres: ["house", "electronic"],
				description: "British electronic musician",
				updatedAt: NOW.toISOString(),
			},
		});
		if (outcome.status !== "enriched") throw new Error("expected enriched profile");
		expect(outcome.profile.links).toEqual([
			{ kind: "resident_advisor", url: "https://ra.co/dj/exampleartist" },
			{ kind: "soundcloud", url: "https://soundcloud.com/example" },
			{ kind: "spotify", url: "https://open.spotify.com/artist/abc" },
			{ kind: "website", url: "https://example.com/" },
		]);
		expect(outcome.profile.provenance).toEqual(
			expect.arrayContaining([
				expect.objectContaining({ field: "description", provider: "wikidata", license: "CC0" }),
				expect.objectContaining({ field: "genres", provider: "musicbrainz" }),
			]),
		);
	});

	it("accepts only one high-scoring exact search match", async () => {
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(
				jsonResponse({
					artists: [
						{ id: MBID, name: "Example Artist", score: 100 },
						{
							id: "11111111-1111-4111-8111-111111111111",
							name: "Different Artist",
							score: 100,
						},
					],
				}),
			)
			.mockResolvedValueOnce(jsonResponse({ id: MBID, name: "Example Artist" }));

		const beforeMusicBrainzRequest = vi.fn(async () => undefined);
		const outcome = await enrichArtist(
			{ festivalId: "festival", setIds: ["set-1"], billing: "Example Artist" },
			{
				userAgent: "Offbeat/Test",
				fetch: fetcher,
				now: () => NOW,
				beforeMusicBrainzRequest,
			},
		);

		expect(outcome).toMatchObject({ status: "enriched", profile: { mbid: MBID } });
		expect(fetcher).toHaveBeenCalledTimes(2);
		expect(beforeMusicBrainzRequest).toHaveBeenCalledTimes(2);
	});

	it("does not treat a diacritic-folded name as an exact identity", async () => {
		const fetcher = vi.fn<typeof fetch>().mockResolvedValueOnce(
			jsonResponse({
				artists: [
					{
						id: "39ca6961-d184-4f7b-be25-2bdab0767bc4",
						name: "Óptimo",
						score: 100,
					},
				],
			}),
		);

		await expect(
			enrichArtist(
				{ festivalId: "houghton2026", setIds: ["set-1"], billing: "Optimo" },
				{ userAgent: "Offbeat/Test", fetch: fetcher },
			),
		).resolves.toEqual({ status: "unresolved", reason: "no_unique_match" });
		expect(fetcher).toHaveBeenCalledOnce();
	});

	it("allows an AI-confirmed legitimate separator act to use exact provider matching", async () => {
		const fetcher = vi
			.fn<typeof fetch>()
			.mockResolvedValueOnce(
				jsonResponse({ artists: [{ id: MBID, name: "Chase & Status", score: 100 }] }),
			)
			.mockResolvedValueOnce(jsonResponse({ id: MBID, name: "Chase & Status" }));

		await expect(
			enrichArtist(
				{ festivalId: "festival", setIds: ["set-1"], billing: "Chase & Status" },
				{
					userAgent: "Offbeat/Test",
					fetch: fetcher,
					allowAmbiguousBilling: true,
				},
			),
		).resolves.toMatchObject({ status: "enriched", profile: { name: "Chase & Status" } });
	});

	it("does not guess when multiple exact candidates remain", async () => {
		const fetcher = vi.fn<typeof fetch>().mockResolvedValueOnce(
			jsonResponse({
				artists: [
					{ id: MBID, name: "Example Artist", score: 100 },
					{
						id: "11111111-1111-4111-8111-111111111111",
						name: "Example Artist",
						score: 99,
					},
				],
			}),
		);

		await expect(
			enrichArtist(
				{ festivalId: "festival", setIds: ["set-1"], billing: "Example Artist" },
				{ userAgent: "Offbeat/Test", fetch: fetcher },
			),
		).resolves.toEqual({ status: "unresolved", reason: "no_unique_match" });
		expect(fetcher).toHaveBeenCalledTimes(1);
	});

	it("rejects an invalid source MBID without provider traffic", async () => {
		const fetcher = vi.fn<typeof fetch>();
		await expect(
			enrichArtist(
				{ festivalId: "festival", setIds: ["set-1"], billing: "Example Artist", mbid: "bad" },
				{ userAgent: "Offbeat/Test", fetch: fetcher },
			),
		).resolves.toEqual({ status: "unresolved", reason: "invalid_mbid" });
		expect(fetcher).not.toHaveBeenCalled();
	});

	it("marks provider throttling as retryable", async () => {
		const fetcher = vi.fn<typeof fetch>().mockResolvedValueOnce(jsonResponse({}, 429));
		await expect(
			enrichArtist(
				{ festivalId: "festival", setIds: ["set-1"], billing: "Example Artist", mbid: MBID },
				{ userAgent: "Offbeat/Test", fetch: fetcher },
			),
		).rejects.toEqual(expect.objectContaining<Partial<ArtistProviderError>>({ retryable: true }));
	});
});
