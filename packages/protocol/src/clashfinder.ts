import { parseArtistBilling } from "./artist-billing.js";
import type { ClashfinderApiEvent, ClashfinderApiResponse } from "./clashfinder-api.js";
import type { Day, Lineup, Set as LineupSet, Stage } from "./types.js";

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
				events: (data as { events?: ClashfinderApiEvent[] }).events ?? [],
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
		artistMbid?: string;
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
				...(event.mbId ? { artistMbid: event.mbId } : {}),
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
	const sortedDayKeys = [...dayMap.keys()].sort((left, right) => left.localeCompare(right));
	const days: Day[] = sortedDayKeys.map((key) => {
		const entry = dayMap.get(key);
		if (!entry) throw new Error(`Day entry not found for key: ${key}`);
		return {
			id: stableId(`day/${key}`),
			label: WEEKDAYS[entry.date.getDay()],
			num: entry.date.getDate(),
			month: MONTHS[entry.date.getMonth()],
			year: entry.date.getFullYear(),
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

		const parsedBilling = parseArtistBilling(event.artist, event.artistMbid);
		return {
			id: stableId(idSource),
			day: dayId,
			stage: event.stage.id,
			artist: event.artist,
			sourceBilling: event.artist,
			...(event.artistMbid ? { artistMbid: event.artistMbid } : {}),
			...(parsedBilling.presentedTitle ? { presentedTitle: parsedBilling.presentedTitle } : {}),
			...(parsedBilling.performanceQualifiers.length > 0
				? { performanceQualifiers: parsedBilling.performanceQualifiers }
				: {}),
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
