/**
 * Clashfinder API client with authentication support.
 *
 * API docs: https://clashfinder.com/page/api/
 */

// --- API Response Types ---

export interface ClashfinderApiResponse {
	locations: ClashfinderLocation[];
	name?: string;
	timezone?: string;
	timezoneOffset?: number;
}

export interface ClashfinderLocation {
	name: string;
	events: ClashfinderApiEvent[];
}

export interface ClashfinderApiEvent {
	name: string;
	short?: string;
	start: string; // "2026-05-23 12:00"
	end: string; // "2026-05-23 13:00"
	mbId?: string; // MusicBrainz ID
}

// --- Source Configuration ---

/**
 * Configuration for a whitelisted clashfinder source.
 * Used to register festivals from known clashfinder events.
 */
export interface ClashfinderSource {
	/** Internal festival ID used in our system */
	festivalId: string;
	/** Clashfinder event name (from URL: clashfinder.com/s/{name}/) */
	clashfinderId: string;
	/** Festival display name */
	name: string;
	/** Festival location */
	location: string;
	city: string;
	country: string;
	/** Genre tags */
	genres: string[];
	/** Venue latitude (WGS84) */
	lat?: number;
	/** Venue longitude (WGS84) */
	lon?: number;
}

// --- Authentication ---

export interface ClashfinderAuth {
	username: string;
	privateKey: string;
}

/**
 * Generate the public key for Clashfinder API authentication.
 *
 * The public key is SHA256(username + privateKey + authParam + authValidUntil).
 * All components are concatenated directly without separators.
 */
export async function generatePublicKey(
	auth: ClashfinderAuth,
	options?: {
		authParam?: string;
		authValidUntil?: string; // yyyymmddhhmmss format
	},
): Promise<string> {
	const parts = [auth.username, auth.privateKey];

	if (options?.authParam) {
		parts.push(options.authParam);
	}
	if (options?.authValidUntil) {
		parts.push(options.authValidUntil);
	}

	const input = parts.join("");
	const encoder = new TextEncoder();
	const data = encoder.encode(input);

	const hashBuffer = await crypto.subtle.digest("SHA-256", data);
	const hashArray = Array.from(new Uint8Array(hashBuffer));
	return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * Build the authenticated URL for a Clashfinder API request.
 */
export function buildApiUrl(
	clashfinderId: string,
	auth: { username: string; publicKey: string },
	options?: {
		authParam?: string;
		authValidUntil?: string;
	},
): string {
	const params = new URLSearchParams({
		authUsername: auth.username,
		authPublicKey: auth.publicKey,
	});

	if (options?.authParam) {
		params.set("authParam", options.authParam);
	}
	if (options?.authValidUntil) {
		params.set("authValidUntil", options.authValidUntil);
	}

	return `https://clashfinder.com/data/event/${clashfinderId}.json?${params.toString()}`;
}

/**
 * Fetch clashfinder data from the API with authentication.
 */
export async function fetchClashfinder(
	clashfinderId: string,
	auth: ClashfinderAuth,
	options?: {
		authParam?: string;
		authValidUntil?: string;
	},
): Promise<ClashfinderApiResponse> {
	const publicKey = await generatePublicKey(auth, options);
	const url = buildApiUrl(clashfinderId, { username: auth.username, publicKey }, options);

	const response = await fetch(url);

	if (!response.ok) {
		throw new Error(`Clashfinder API error: ${response.status} ${response.statusText}`);
	}

	return response.json() as Promise<ClashfinderApiResponse>;
}
