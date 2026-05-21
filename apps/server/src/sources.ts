import type { ClashfinderSource } from "@offbeat/protocol";

/**
 * Whitelist of clashfinder sources for festivals.
 *
 * When registering a new festival, use one of these configurations
 * to pull lineup data from the Clashfinder API.
 */
export const FESTIVAL_SOURCES: ClashfinderSource[] = [
	{
		festivalId: "fieldday2026",
		clashfinderId: "fieldday2026",
		name: "Field Day 2026",
		location: "Victoria Park, London",
		city: "London",
		country: "GB",
		genres: ["Electronic", "Indie", "Experimental"],
	},
	{
		festivalId: "gala2026",
		clashfinderId: "gala2o26",
		name: "GALA 2026",
		location: "Peckham Rye Park, London",
		city: "London",
		country: "GB",
		genres: ["Electronic", "House", "Techno"],
	},
	{
		festivalId: "houghton2025",
		clashfinderId: "houghton25",
		name: "Houghton 2025",
		location: "Houghton Hall, Norfolk",
		city: "King's Lynn",
		country: "GB",
		genres: ["Electronic", "House", "Techno", "Ambient"],
	},
];

/**
 * Get a festival source by its internal ID.
 */
export function getSource(festivalId: string): ClashfinderSource | undefined {
	return FESTIVAL_SOURCES.find((s) => s.festivalId === festivalId);
}

/**
 * Get a festival source by its clashfinder ID.
 */
export function getSourceByClashfinderId(clashfinderId: string): ClashfinderSource | undefined {
	return FESTIVAL_SOURCES.find((s) => s.clashfinderId === clashfinderId);
}
