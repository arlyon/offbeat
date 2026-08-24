const DAY_MS = 24 * 60 * 60 * 1000;
const MAX_FORECAST_DAYS = 16;
const MONTH_INDEX = new Map(
	["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].map(
		(month, index) => [month, index],
	),
);

export interface FestivalCalendarDay {
	num: number;
	month: string;
	year: number;
}

export interface FestivalWeatherWindow {
	startsAt: string;
	endsAt: string;
	fetchStartsAt: string;
}

function calendarDayUtc(day: FestivalCalendarDay): number {
	const month = MONTH_INDEX.get(day.month);
	if (month === undefined || !Number.isInteger(day.num) || !Number.isInteger(day.year)) {
		throw new Error("Festival lineup contains an invalid calendar day");
	}
	const value = Date.UTC(day.year, month, day.num);
	const parsed = new Date(value);
	if (
		parsed.getUTCFullYear() !== day.year ||
		parsed.getUTCMonth() !== month ||
		parsed.getUTCDate() !== day.num
	) {
		throw new Error("Festival lineup contains an invalid calendar day");
	}
	return value;
}

/** Build the attendee weather window from the first and last programmed lineup days. */
export function festivalWeatherWindow(days: FestivalCalendarDay[]): FestivalWeatherWindow {
	if (days.length === 0) throw new Error("Festival lineup has no programmed days");
	const timestamps = days.map(calendarDayUtc);
	const firstDay = Math.min(...timestamps);
	const lastDay = Math.max(...timestamps);
	const startsAt = firstDay - DAY_MS;
	const endsAt = lastDay + 2 * DAY_MS - 1;

	return {
		startsAt: new Date(startsAt).toISOString(),
		endsAt: new Date(endsAt).toISOString(),
		fetchStartsAt: new Date(startsAt - 7 * DAY_MS).toISOString(),
	};
}

/** Number of inclusive forecast days needed from today, capped by Open-Meteo's limit. */
export function forecastDaysThrough(windowEnd: string, now = new Date()): number {
	const end = new Date(windowEnd).getTime();
	if (!Number.isFinite(end)) throw new Error("Weather window end is invalid");
	const today = Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
	return Math.max(1, Math.min(MAX_FORECAST_DAYS, Math.ceil((end - today + 1) / DAY_MS)));
}
