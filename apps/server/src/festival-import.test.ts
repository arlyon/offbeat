import type { ClashfinderApiResponse } from "@offbeat/protocol";
import { describe, expect, it } from "vitest";
import {
	festivalImportSigningPayload,
	normalizeClashfinderId,
	validateClashfinderImport,
} from "./festival-import";

const fixture: ClashfinderApiResponse = {
	name: "Example Festival",
	locations: [
		{
			name: "Main Stage",
			events: [
				{
					name: "Artist One",
					start: "2027-06-12 12:00",
					end: "2027-06-12 13:00",
				},
			],
		},
	],
};

describe("festival import", () => {
	it("normalizes only safe Clashfinder IDs and canonical URLs", () => {
		expect(normalizeClashfinderId("Example_2027")).toBe("example_2027");
		expect(normalizeClashfinderId("https://clashfinder.com/s/Example_2027/")).toBe(
			"example_2027",
		);
		expect(normalizeClashfinderId("https://example.com/s/event/")).toBeNull();
		expect(normalizeClashfinderId("https://clashfinder.com/data/event/test.json")).toBeNull();
		expect(normalizeClashfinderId("../../admin")).toBeNull();
	});

	it("binds request method, path, freshness fields, and exact body", () => {
		expect(
			festivalImportSigningPayload(
				"post",
				"/festival-imports/preview",
				"123",
				"abc",
				'{"clashfinder":"event"}',
			),
		).toBe(
			'offbeat:festival-import:v1\nPOST\n/festival-imports/preview\n123\nabc\n{"clashfinder":"event"}',
		);
	});

	it("derives deterministic event metadata and a complete lineup", () => {
		const result = validateClashfinderImport("example2027", fixture);
		expect(result).toMatchObject({
			clashfinderId: "example2027",
			festivalId: "cf-example2027",
			name: "Example Festival",
			startDate: "2027-06-12",
			endDate: "2027-06-12",
			year: 2027,
			stageCount: 1,
			setCount: 1,
		});
		expect(result.lineup.sets).toHaveLength(1);
	});

	it("rejects empty and invalid schedules", () => {
		expect(() => validateClashfinderImport("empty", { locations: [] })).toThrow(
			"must contain 1-100 stages",
		);
		expect(() =>
			validateClashfinderImport("duplicate-stage", {
				locations: [fixture.locations[0], fixture.locations[0]],
			}),
		).toThrow("duplicate stages");
		expect(() =>
			validateClashfinderImport("bad-time", {
				locations: [
					{
						name: "Stage",
						events: [{ name: "Artist", start: "bad", end: "also bad" }],
					},
				],
			}),
		).toThrow("set time is invalid");
	});
});
