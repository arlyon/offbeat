import type { ClashfinderSource } from "@offbeat/protocol";

/**
 * Festival sources for Clashfinder integration.
 *
 * These map internal festival IDs to Clashfinder IDs for fetching lineups.
 * Source data is defined in fixtures/*.json files.
 *
 * To add a new festival:
 * 1. Create a JSON file in fixtures/ with the festival metadata
 * 2. Include a `clashfinderId` field if it has a Clashfinder page
 * 3. Run: pnpm -F @offbeat/server festival:seed
 */

// Load sources from fixture data at build time
// For runtime, we maintain a simple lookup based on known festivals
const SOURCES: ClashfinderSource[] = [
	{
		festivalId: "fieldday2026",
		clashfinderId: "fieldday2026",
		name: "Field Day 2026",
		location: "Victoria Park, London",
		city: "London",
		country: "GB",
		genres: ["Electronic", "Indie", "Experimental"],
		lat: 51.5369,
		lon: -0.0394,
	},
	{
		festivalId: "gala2026",
		clashfinderId: "gala2o26",
		name: "GALA 2026",
		location: "Peckham Rye Park, London",
		city: "London",
		country: "GB",
		genres: ["Electronic", "House", "Techno"],
		lat: 51.4625,
		lon: -0.0693,
	},
	{
		festivalId: "houghton2025",
		clashfinderId: "houghton25",
		name: "Houghton 2025",
		location: "Houghton Hall, Norfolk",
		city: "King's Lynn",
		country: "GB",
		genres: ["Electronic", "House", "Techno", "Ambient"],
		lat: 52.8272,
		lon: 0.6544,
	},
];

/**
 * Get all known festival sources.
 */
export function getAllSources(): ClashfinderSource[] {
	return SOURCES;
}

/**
 * Get a festival source by its internal ID.
 */
export function getSource(festivalId: string): ClashfinderSource | undefined {
	return SOURCES.find((s) => s.festivalId === festivalId);
}

/**
 * Get a festival source by its clashfinder ID.
 */
export function getSourceByClashfinderId(clashfinderId: string): ClashfinderSource | undefined {
	return SOURCES.find((s) => s.clashfinderId === clashfinderId);
}
