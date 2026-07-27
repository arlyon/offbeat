import type { ClashfinderApiResponse, Lineup } from "@offbeat/protocol";
import { parseClashfinderApi } from "@offbeat/protocol";

export const IMPORT_PREVIEW_TTL_SECONDS = 15 * 60;
export const IMPORT_PREVIEW_LIMIT = 10;
export const IMPORT_PREVIEW_WINDOW_SECONDS = 60 * 60;
export const IMPORT_PUBLISH_LIMIT = 3;
export const IMPORT_PUBLISH_WINDOW_SECONDS = 24 * 60 * 60;
export const IMPORT_NETWORK_PREVIEW_LIMIT = 20;
export const IMPORT_NETWORK_PUBLISH_LIMIT = 5;
export const IMPORT_GLOBAL_PREVIEW_LIMIT = 100;
export const IMPORT_GLOBAL_PUBLISH_LIMIT = 30;
export const MAX_CLASHFINDER_RESPONSE_BYTES = 2 * 1024 * 1024;
export const MAX_IMPORT_REQUEST_BYTES = 64 * 1024;
export const MAX_IMPORT_STAGES = 100;
export const MAX_IMPORT_SETS = 5_000;
export const IMPORT_REQUEST_MAX_SKEW_SECONDS = 5 * 60;

export interface ValidatedClashfinderImport {
	clashfinderId: string;
	festivalId: string;
	name: string;
	startDate: string;
	endDate: string;
	year: number;
	stageCount: number;
	setCount: number;
	lineup: Lineup;
}

export function normalizeClashfinderId(input: string): string | null {
	let candidate = input.trim();
	if (!candidate) return null;

	if (candidate.includes("://")) {
		let url: URL;
		try {
			url = new URL(candidate);
		} catch {
			return null;
		}
		if (
			url.protocol !== "https:" ||
			!["clashfinder.com", "www.clashfinder.com"].includes(url.hostname)
		) {
			return null;
		}
		const match = url.pathname.match(/^\/s\/([^/]+)\/?$/);
		if (!match) return null;
		try {
			candidate = decodeURIComponent(match[1]);
		} catch {
			return null;
		}
	}

	if (!/^[a-zA-Z0-9][a-zA-Z0-9_-]{0,63}$/.test(candidate)) return null;
	return candidate.toLowerCase();
}

export function festivalImportSigningPayload(
	method: string,
	path: string,
	timestamp: string,
	nonce: string,
	body: string,
): string {
	return ["offbeat:festival-import:v1", method.toUpperCase(), path, timestamp, nonce, body].join(
		"\n",
	);
}

function parseDate(value: string): Date | null {
	if (!/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/.test(value)) return null;
	const date = new Date(value.replace(" ", "T"));
	return Number.isNaN(date.getTime()) ? null : date;
}

export function validateClashfinderImport(
	clashfinderId: string,
	response: ClashfinderApiResponse,
): ValidatedClashfinderImport {
	if (!Array.isArray(response.locations)) {
		throw new Error("Clashfinder response has no stage list");
	}
	if (response.locations.length === 0 || response.locations.length > MAX_IMPORT_STAGES) {
		throw new Error(`Clashfinder event must contain 1-${MAX_IMPORT_STAGES} stages`);
	}

	const events = response.locations.flatMap((location) => {
		if (!location || typeof location.name !== "string" || !Array.isArray(location.events)) {
			throw new Error("Clashfinder response contains an invalid stage");
		}
		if (!location.name.trim() || location.name.length > 200) {
			throw new Error("Clashfinder stage name is invalid");
		}
		return location.events;
	});
	if (events.length === 0 || events.length > MAX_IMPORT_SETS) {
		throw new Error(`Clashfinder event must contain 1-${MAX_IMPORT_SETS} sets`);
	}

	const datetimes: Date[] = [];
	for (const event of events) {
		if (!event || typeof event.name !== "string" || !event.name.trim() || event.name.length > 300) {
			throw new Error("Clashfinder set name is invalid");
		}
		const start = parseDate(event.start);
		const end = parseDate(event.end);
		if (!start || !end || end.getTime() <= start.getTime()) {
			throw new Error("Clashfinder set time is invalid");
		}
		datetimes.push(start, end);
	}
	datetimes.sort((left, right) => left.getTime() - right.getTime());

	const start = datetimes[0];
	const end = datetimes[datetimes.length - 1];
	if (end.getTime() - start.getTime() > 31 * 24 * 60 * 60 * 1000) {
		throw new Error("Clashfinder event spans more than 31 days");
	}

	const festivalId = `cf-${clashfinderId}`;
	const name = response.name?.trim() || clashfinderId;
	if (name.length > 200) throw new Error("Clashfinder event name is too long");
	const lineup = parseClashfinderApi(festivalId, response, { name, location: "" });
	if (lineup.stages.length !== response.locations.length || lineup.sets.length !== events.length) {
		throw new Error("Clashfinder event could not be parsed completely");
	}
	if (new Set(lineup.stages.map((stage) => stage.id)).size !== lineup.stages.length) {
		throw new Error("Clashfinder event contains duplicate stages");
	}
	if (new Set(lineup.sets.map((set) => set.id)).size !== lineup.sets.length) {
		throw new Error("Clashfinder event contains duplicate sets");
	}

	return {
		clashfinderId,
		festivalId,
		name,
		startDate: start.toISOString().split("T")[0],
		endDate: end.toISOString().split("T")[0],
		year: start.getFullYear(),
		stageCount: lineup.stages.length,
		setCount: lineup.sets.length,
		lineup,
	};
}
