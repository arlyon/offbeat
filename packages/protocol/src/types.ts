export interface Festival {
	id: string;
	name: string;
	year: number;
	location: string;
	city: string;
	country: string;
	startDate: string; // ISO string
	endDate: string; // ISO string
	stages: Stage[];
	genres: string[];
	status: "upcoming" | "live" | "past";
	clashfinderId?: string; // Clashfinder event ID for lineup refresh
	publicKey: string;
	updatedAt: string; // ISO string
}

export interface Stage {
	id: string;
	name: string;
	short: string; // abbreviated name
	color: string; // hex string
	order: number;
}

export interface Day {
	id: string;
	label: string; // e.g. "Friday"
	num: number; // day-of-month
	month: string; // e.g. "Jun"
	year: number; // e.g. 2026
}

export type ArtistLinkKind =
	| "website"
	| "spotify"
	| "soundcloud"
	| "youtube"
	| "instagram"
	| "facebook"
	| "x"
	| "other";

export interface ArtistLink {
	kind: ArtistLinkKind;
	url: string;
}

export interface ArtistFieldProvenance {
	field: string;
	provider: "musicbrainz" | "wikidata" | "festival";
	sourceUrl: string;
	license: string;
	retrievedAt: string;
}

export interface ArtistProfile {
	id: string;
	name: string;
	mbid: string;
	wikidataId?: string;
	aliases: string[];
	artistType?: string;
	country?: string;
	genres: string[];
	description?: string;
	links: ArtistLink[];
	provenance: ArtistFieldProvenance[];
	updatedAt: string;
}

export interface Set {
	id: string;
	day: string; // Day id ref
	stage: string; // Stage id ref
	artist: string;
	artistMbid?: string;
	artistIds?: string[];
	startMin: number; // minutes from midnight
	durationMin: number;
	genre: string;
	cancelled: boolean;
}

export interface Lineup {
	festival: Pick<Festival, "id" | "name" | "location">;
	stages: Stage[];
	days: Day[];
	sets: Set[];
	artists?: ArtistProfile[];
}

export interface MemberLocation {
	userId: string;
	displayName: string;
	stageId: string | null;
	customLocation: string | null;
	status: "active" | "idle";
	updatedAt: string; // ISO string
}

export interface GroupPin {
	id: string;
	label: string;
	location: string;
	pinnedBy: string;
	createdAt: string; // ISO string
}

export interface ChatMessage {
	id: string;
	userId: string;
	displayName: string;
	text: string;
	topic: string;
	stageId: string | null;
	timestamp: string; // ISO string
}

export interface TransportStatus {
	mode: "full" | "local" | "mesh" | "offline";
	wsConnected: boolean;
	blePeers: number;
	meshConnected: boolean;
}

// SignedUpdate is now generated from proto — see generated/offbeat/v1/types_pb.ts
// The proto version uses raw bytes instead of base64-encoded strings.
