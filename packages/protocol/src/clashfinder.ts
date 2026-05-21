import type { ClashfinderApiResponse } from "./clashfinder-api.js";
import type { Day, Lineup, Set as LineupSet, Stage } from "./types.js";

/**
 * Legacy event format (for local fixtures/testing).
 */
export interface ClashfinderEvent {
	artist: string;
	stage: string;
	day: string; // "friday", "saturday"
	start: string; // "18:00"
	end: string; // "19:30"
}

const STAGE_COLORS = ["#FF2D8F", "#3DDBD9", "#FFB347", "#9BE15D", "#C77DFF", "#FF8C42"];

const WEEKDAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/**
 * Simple stable hash for generating IDs from content strings.
 * Returns a hex string of 8 characters.
 */
function stableId(input: string): string {
	let hash = 5381;
	for (let i = 0; i < input.length; i++) {
		hash = ((hash << 5) + hash) ^ input.charCodeAt(i);
		hash = hash >>> 0; // keep as unsigned 32-bit
	}
	return hash.toString(16).padStart(8, "0");
}

/**
 * Parse a time string like "18:00" to minutes from midnight.
 * Times are taken at face value (no overnight wrapping here).
 */
export function parseTimeToMinutes(time: string): number {
	const [hourStr, minStr] = time.split(":");
	const hours = Number.parseInt(hourStr, 10);
	const minutes = Number.parseInt(minStr, 10);
	return hours * 60 + minutes;
}

/**
 * Calculate duration in minutes between start and end times.
 * Handles overnight: if end < start, adds 1440 minutes (24h).
 */
export function calcDurationMin(start: string, end: string): number {
	const startMin = parseTimeToMinutes(start);
	const endMin = parseTimeToMinutes(end);
	if (endMin < startMin) {
		return endMin + 1440 - startMin;
	}
	return endMin - startMin;
}

// Day name to num/month mapping for a June festival
const DAY_INFO: Record<string, { num: number; month: string }> = {
	friday: { num: 13, month: "Jun" },
	saturday: { num: 14, month: "Jun" },
	sunday: { num: 15, month: "Jun" },
	thursday: { num: 12, month: "Jun" },
	monday: { num: 16, month: "Jun" },
};

export function parseClashfinder(festivalId: string, events: ClashfinderEvent[]): Lineup {
	// Derive unique stages in order of first appearance
	const stageOrder: string[] = [];
	const stageSeen = new globalThis.Set<string>();
	for (const event of events) {
		if (!stageSeen.has(event.stage)) {
			stageSeen.add(event.stage);
			stageOrder.push(event.stage);
		}
	}

	const stages: Stage[] = stageOrder.map((name, idx) => ({
		id: stableId(`stage/${name}`),
		name,
		short: name.slice(0, 3).toUpperCase(),
		color: STAGE_COLORS[idx % STAGE_COLORS.length],
		order: idx,
	}));

	const stageByName = new Map(stages.map((s) => [s.name, s]));

	// Derive unique days in order of first appearance
	const dayOrder: string[] = [];
	const daySeen = new globalThis.Set<string>();
	for (const event of events) {
		const dayKey = event.day.toLowerCase();
		if (!daySeen.has(dayKey)) {
			daySeen.add(dayKey);
			dayOrder.push(dayKey);
		}
	}

	const days: Day[] = dayOrder.map((dayKey) => {
		const info = DAY_INFO[dayKey] ?? { num: 1, month: "Jun" };
		return {
			id: stableId(`day/${dayKey}`),
			label: dayKey.charAt(0).toUpperCase() + dayKey.slice(1),
			num: info.num,
			month: info.month,
		};
	});

	const dayByKey = new Map(days.map((d) => [d.label.toLowerCase(), d]));

	// Build sets
	const sets: LineupSet[] = events.map((event) => {
		const dayKey = event.day.toLowerCase();
		const day = dayByKey.get(dayKey);
		const stage = stageByName.get(event.stage);

		if (!day) throw new Error(`Unknown day: ${event.day}`);
		if (!stage) throw new Error(`Unknown stage: ${event.stage}`);

		const idSource =
			`${festivalId}/${dayKey}/${event.stage}/${event.artist}/${event.start}`.replace(/\//g, "-");

		return {
			id: stableId(idSource),
			day: day.id,
			stage: stage.id,
			artist: event.artist,
			startMin: parseTimeToMinutes(event.start),
			durationMin: calcDurationMin(event.start, event.end),
			genre: "",
			cancelled: false,
		};
	});

	return {
		festival: {
			id: festivalId,
			name: festivalId,
			location: "",
		},
		stages,
		days,
		sets,
	};
}

/**
 * Parse a datetime string from the Clashfinder API.
 * Format: "2026-05-23 12:00"
 */
function parseApiDatetime(datetime: string): { date: Date; minutes: number } {
	const [datePart, timePart] = datetime.split(" ");
	const [year, month, day] = datePart.split("-").map(Number);
	const [hours, minutes] = timePart.split(":").map(Number);

	const date = new Date(year, month - 1, day, hours, minutes);
	const minutesFromMidnight = hours * 60 + minutes;

	return { date, minutes: minutesFromMidnight };
}

/**
 * Get a unique day key from a date (used for grouping events by day).
 */
function getDayKey(date: Date): string {
	return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

/**
 * Parse a Clashfinder API response into a Lineup.
 *
 * This handles the real API format where events are nested under locations (stages),
 * and times are full ISO-ish datetimes like "2026-05-23 12:00".
 */
export function parseClashfinderApi(
	festivalId: string,
	response: ClashfinderApiResponse,
	meta?: { name?: string; location?: string },
): Lineup {
	// Handle both array and object formats for locations
	const locationsArray = Array.isArray(response.locations)
		? response.locations
		: Object.entries(response.locations).map(([name, data]) => ({
				name,
				events: (data as { events?: ClashfinderEvent[] }).events ?? [],
			}));

	// Build stages from locations in order
	const stages: Stage[] = locationsArray.map((loc, idx) => ({
		id: stableId(`stage/${loc.name}`),
		name: String(loc.name),
		short: String(loc.name).slice(0, 3).toUpperCase(),
		color: STAGE_COLORS[idx % STAGE_COLORS.length],
		order: idx,
	}));

	const stageByName = new Map(stages.map((s) => [s.name, s]));

	// Collect all events with their stage reference
	const allEvents: Array<{
		artist: string;
		stage: Stage;
		start: string;
		end: string;
	}> = [];

	for (const location of response.locations) {
		const stage = stageByName.get(location.name);
		if (!stage) continue;

		for (const event of location.events) {
			allEvents.push({
				artist: event.name,
				stage,
				start: event.start,
				end: event.end,
			});
		}
	}

	// Derive unique days from event start times
	const dayMap = new Map<string, { date: Date; events: typeof allEvents }>();

	for (const event of allEvents) {
		const { date } = parseApiDatetime(event.start);
		const key = getDayKey(date);

		let entry = dayMap.get(key);
		if (!entry) {
			entry = { date, events: [] };
			dayMap.set(key, entry);
		}
		entry.events.push(event);
	}

	// Sort days chronologically and build Day objects
	const sortedDayKeys = [...dayMap.keys()].sort();
	const days: Day[] = sortedDayKeys.map((key) => {
		const entry = dayMap.get(key);
		if (!entry) throw new Error(`Day entry not found for key: ${key}`);
		return {
			id: stableId(`day/${key}`),
			label: WEEKDAYS[entry.date.getDay()],
			num: entry.date.getDate(),
			month: MONTHS[entry.date.getMonth()],
		};
	});

	const dayIdByKey = new Map(sortedDayKeys.map((key, i) => [key, days[i].id]));

	// Build sets
	const sets: LineupSet[] = allEvents.map((event) => {
		const { date: startDate, minutes: startMin } = parseApiDatetime(event.start);
		const { minutes: endMin } = parseApiDatetime(event.end);

		const dayKey = getDayKey(startDate);
		const dayId = dayIdByKey.get(dayKey);
		if (!dayId) throw new Error(`Day ID not found for key: ${dayKey}`);

		// Calculate duration, handling overnight
		let durationMin = endMin - startMin;
		if (durationMin < 0) {
			durationMin += 1440; // add 24 hours
		}

		const idSource =
			`${festivalId}/${dayKey}/${event.stage.name}/${event.artist}/${event.start}`.replace(
				/[\s/]/g,
				"-",
			);

		return {
			id: stableId(idSource),
			day: dayId,
			stage: event.stage.id,
			artist: event.artist,
			startMin,
			durationMin,
			genre: "",
			cancelled: false,
		};
	});

	return {
		festival: {
			id: festivalId,
			name: meta?.name ?? response.name ?? festivalId,
			location: meta?.location ?? "",
		},
		stages,
		days,
		sets,
	};
}
