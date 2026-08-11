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
	| "resident_advisor"
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
	provider: "musicbrainz" | "wikidata" | "resident_advisor" | "festival" | "admin";
	sourceUrl: string;
	license: string;
	retrievedAt: string;
}

export interface ArtistRelation {
	kind: "member_of";
	artistId: string;
}

export interface ArtistProfile {
	id: string;
	name: string;
	mbid?: string;
	wikidataId?: string;
	aliases: string[];
	artistType?: string;
	country?: string;
	genres: string[];
	description?: string;
	links: ArtistLink[];
	relations?: ArtistRelation[];
	provenance: ArtistFieldProvenance[];
	updatedAt: string;
}

export type ArtistCreditRole = "performer" | "presenter" | "guest";

export type PerformanceQualifier =
	| "dj_set"
	| "live"
	| "ambient_set"
	| "hybrid_set"
	| "reggae_set"
	| "balearic_set"
	| "electro_set"
	| "r_and_b_set"
	| "solo_piano"
	| "live_keyboard";

export interface ArtistCredit {
	artistId: string;
	canonicalName: string;
	creditedAs: string;
	role: ArtistCreditRole;
}

export interface ArtistCreditProposal {
	canonicalName: string;
	creditedAs: string;
	role: ArtistCreditRole;
	confidence: number;
}

export interface ArtistResolutionEvidence {
	url: string;
	title: string;
	claims: Array<"alias" | "act_identity" | "event_billing">;
	retrievedAt: string;
}

export interface ArtistBillingResolution {
	id: string;
	sourceBilling: string;
	billingKey: string;
	status: "resolved" | "needs_review" | "unresolved";
	method: "deterministic" | "ai" | "manual" | "legacy";
	confidence: number;
	credits: ArtistCredit[];
	proposedCredits?: ArtistCreditProposal[];
	presentedTitle?: string;
	performanceQualifiers: PerformanceQualifier[];
	evidence: ArtistResolutionEvidence[];
	inputHash: string;
	processorVersion: string;
	model?: string;
	version: number;
}

export interface Set {
	id: string;
	day: string; // Day id ref
	stage: string; // Stage id ref
	/** Exact source billing retained for backward-compatible display. */
	artist: string;
	/** Explicit copy of the exact source billing for newer clients. */
	sourceBilling?: string;
	artistMbid?: string;
	artistIds?: string[];
	billingResolutionId?: string;
	artistCredits?: ArtistCredit[];
	presentedTitle?: string;
	performanceQualifiers?: PerformanceQualifier[];
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
	billingResolutions?: ArtistBillingResolution[];
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
