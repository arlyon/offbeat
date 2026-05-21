import { describe, expect, it } from "vitest";
import fieldday26 from "../fixtures/fieldday26.json";
import {
	type ClashfinderEvent,
	calcDurationMin,
	parseClashfinder,
	parseTimeToMinutes,
} from "./clashfinder.js";

const events = fieldday26 as ClashfinderEvent[];

describe("parseClashfinder", () => {
	it("parses the correct number of stages (6)", () => {
		const lineup = parseClashfinder("fieldday26", events);
		expect(lineup.stages).toHaveLength(6);
	});

	it("parses the correct number of days (2)", () => {
		const lineup = parseClashfinder("fieldday26", events);
		expect(lineup.days).toHaveLength(2);
	});

	it("parses all sets (~30)", () => {
		const lineup = parseClashfinder("fieldday26", events);
		expect(lineup.sets).toHaveLength(events.length);
		expect(lineup.sets.length).toBeGreaterThanOrEqual(30);
	});

	it("set IDs are stable (parse twice, compare)", () => {
		const lineup1 = parseClashfinder("fieldday26", events);
		const lineup2 = parseClashfinder("fieldday26", events);
		const ids1 = lineup1.sets.map((s) => s.id).sort();
		const ids2 = lineup2.sets.map((s) => s.id).sort();
		expect(ids1).toEqual(ids2);
	});
});

describe("parseTimeToMinutes", () => {
	it('parses "18:00" → 1080', () => {
		expect(parseTimeToMinutes("18:00")).toBe(1080);
	});

	it('parses "00:30" → 30', () => {
		expect(parseTimeToMinutes("00:30")).toBe(30);
	});

	it('parses "01:30" → 90', () => {
		expect(parseTimeToMinutes("01:30")).toBe(90);
	});
});

describe("calcDurationMin", () => {
	it("calculates 18:00-19:30 → 90min", () => {
		expect(calcDurationMin("18:00", "19:30")).toBe(90);
	});

	it("calculates overnight 23:00-01:00 → 120min", () => {
		expect(calcDurationMin("23:00", "01:00")).toBe(120);
	});

	it("calculates 22:00-00:00 → 120min (overnight)", () => {
		expect(calcDurationMin("22:00", "00:00")).toBe(120);
	});
});
