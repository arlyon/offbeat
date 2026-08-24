import { describe, expect, it } from "vitest";
import { festivalWeatherWindow, forecastDaysThrough } from "./weather-window";

describe("festivalWeatherWindow", () => {
	it("covers Wednesday through Monday for a Thursday through Sunday lineup", () => {
		expect(
			festivalWeatherWindow([
				{ num: 29, month: "Aug", year: 2026 },
				{ num: 27, month: "Aug", year: 2026 },
				{ num: 30, month: "Aug", year: 2026 },
				{ num: 28, month: "Aug", year: 2026 },
			]),
		).toEqual({
			startsAt: "2026-08-26T00:00:00.000Z",
			endsAt: "2026-08-31T23:59:59.999Z",
			fetchStartsAt: "2026-08-19T00:00:00.000Z",
		});
	});

	it("rejects missing and invalid lineup days", () => {
		expect(() => festivalWeatherWindow([])).toThrow("no programmed days");
		expect(() => festivalWeatherWindow([{ num: 31, month: "Feb", year: 2026 }])).toThrow(
			"invalid calendar day",
		);
	});
});

describe("forecastDaysThrough", () => {
	it("requests the inclusive number of days through the weather window", () => {
		expect(forecastDaysThrough("2026-08-31T23:59:59.999Z", new Date("2026-08-21T12:00:00Z"))).toBe(
			11,
		);
	});

	it("caps requests at Open-Meteo's 16-day forecast limit", () => {
		expect(forecastDaysThrough("2026-09-30T23:59:59.999Z", new Date("2026-08-21T12:00:00Z"))).toBe(
			16,
		);
	});
});
