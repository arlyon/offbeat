import type { Day, Lineup, Set as LineupSet, Stage } from "./types.js";

export interface ClashfinderEvent {
	artist: string;
	stage: string;
	day: string; // "friday", "saturday"
	start: string; // "18:00"
	end: string; // "19:30"
}

const STAGE_COLORS = ["#FF2D8F", "#3DDBD9", "#FFB347", "#9BE15D", "#C77DFF", "#FF8C42"];

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
