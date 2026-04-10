// All fields in snake_case matching Rust serde defaults

export interface WorldSnapshot {
	location_name: string;
	location_description: string;
	time_label: string;
	hour: number;
	minute: number;
	weather: string;
	season: string;
	festival: string | null;
	paused: boolean;
	game_epoch_ms: number;
	speed_factor: number;
	name_hints: LanguageHint[];
	day_of_week: string;
}

export interface MapLocation {
	id: string;
	name: string;
	lat: number;
	lon: number;
	adjacent: boolean;
	hops: number;
	indoor?: boolean;
	travel_minutes?: number;
	/** Whether the player has visited this location (false = fog-of-war frontier). */
	visited?: boolean;
}

export interface MapData {
	locations: MapLocation[];
	edges: [string, string][];
	player_location: string;
	player_lat: number;
	player_lon: number;
	/** Edge traversal counts for footprint rendering: [src_id, dst_id, count]. */
	edge_traversals?: [string, string, number][];
	/** Human-readable transport mode label (e.g. "on foot"). */
	transport_label?: string;
	/** Machine identifier for the active transport mode (e.g. "walking"). */
	transport_id?: string;
}

/** A waypoint along a travel path. */
export interface TravelWaypoint {
	id: string;
	lat: number;
	lon: number;
}

/** Payload for travel-start events (animated travel on the map). */
export interface TravelStartPayload {
	waypoints: TravelWaypoint[];
	duration_minutes: number;
	destination: string;
}

export interface NpcInfo {
	name: string;
	/** Canonical real name, used as a stable id for chip dispatch. */
	real_name: string;
	occupation: string;
	mood: string;
	introduced: boolean;
	mood_emoji: string;
}

export interface ThemePalette {
	bg: string;
	fg: string;
	accent: string;
	panel_bg: string;
	input_bg: string;
	border: string;
	muted: string;
}

export interface LanguageHint {
	word: string;
	pronunciation: string;
	meaning: string | null;
}

/** Backward-compatible alias. */
export type IrishWordHint = LanguageHint;

export interface UiConfig {
	hints_label: string;
	default_accent: string;
	splash_text: string;
}

export interface Reaction {
	emoji: string;
	source: string;
}

export interface TextLogEntry {
	id?: string;
	source: string;
	content: string;
	streaming?: boolean;
	latest_chunk?: string;
	stream_chunk_id?: number;
	reactions?: Reaction[];
}

export interface StreamTokenPayload {
	token: string;
}

export interface StreamEndPayload {
	hints: IrishWordHint[];
}

export interface TextLogPayload {
	id?: string;
	source: string;
	content: string;
}

export interface NpcReactionPayload {
	message_id: string;
	emoji: string;
	source: string;
}

export type WorldUpdatePayload = WorldSnapshot;

export interface LoadingPayload {
	active: boolean;
	spinner?: string;
	phrase?: string;
	color?: [number, number, number];
}

// ── Debug types ─────────────────────────────────────────────────────────────

export interface DebugSnapshot {
	clock: ClockDebug;
	world: WorldDebug;
	npcs: NpcDebug[];
	tier_summary: TierSummary;
	events: DebugEvent[];
	inference: InferenceDebug;
	gossip: GossipDebug;
}

export interface ClockDebug {
	game_time: string;
	time_of_day: string;
	season: string;
	festival: string | null;
	weather: string;
	paused: boolean;
	speed_factor: number;
	day_of_week: string;
	day_type: string;
	/** Last ~5 weather transitions: [iso_timestamp, weather_label]. */
	weather_recent: [string, string][];
}

export interface GossipDebug {
	rumor_count: number;
	recent_witnesses: number;
	top_rumors: string[];
}

export interface WorldDebug {
	player_location_name: string;
	player_location_id: number;
	location_count: number;
	locations: LocationDebug[];
}

export interface LocationDebug {
	id: number;
	name: string;
	indoor: boolean;
	public: boolean;
	connection_count: number;
	npcs_here: string[];
}

export interface NpcDebug {
	id: number;
	name: string;
	age: number;
	occupation: string;
	personality: string;
	location_name: string;
	location_id: number;
	home_name: string | null;
	workplace_name: string | null;
	mood: string;
	state: string;
	tier: string;
	schedule: ScheduleVariantDebug[];
	relationships: RelationshipDebug[];
	memories: MemoryDebug[];
	knowledge: string[];
	intelligence: IntelligenceDebug;
	last_activity: string | null;
	is_ill: boolean;
	deflated_summary: string | null;
	long_term_memory_count: number;
}

export interface ScheduleVariantDebug {
	season: string | null;
	day_type: string | null;
	is_active: boolean;
	entries: ScheduleEntryDebug[];
}

export interface IntelligenceDebug {
	verbal: number;
	analytical: number;
	emotional: number;
	practical: number;
	wisdom: number;
	creative: number;
}

export interface ScheduleEntryDebug {
	start_hour: number;
	end_hour: number;
	location_name: string;
	activity: string;
	is_current: boolean;
}

export interface RelationshipDebug {
	target_name: string;
	kind: string;
	strength: number;
	history_count: number;
}

export interface MemoryDebug {
	timestamp: string;
	content: string;
	location_name: string;
}

export interface TierSummary {
	tier1_count: number;
	tier2_count: number;
	tier3_count: number;
	tier4_count: number;
	tier1_names: string[];
	tier2_names: string[];
	tier3_names: string[];
	tier3_in_flight: boolean;
	last_tier3_tick: string | null;
	tier2_in_flight: boolean;
	last_tier2_tick: string | null;
	tier3_pending_count: number;
	tier4_recent_events: string[];
}

export interface DebugEvent {
	timestamp: string;
	category: string;
	message: string;
}

export interface InferenceDebug {
	provider_name: string;
	model_name: string;
	base_url: string;
	cloud_provider: string | null;
	cloud_model: string | null;
	has_queue: boolean;
	improv_enabled: boolean;
	call_log: InferenceLogEntry[];
}

export interface InferenceLogEntry {
	request_id: number;
	timestamp: string;
	model: string;
	streaming: boolean;
	duration_ms: number;
	prompt_len: number;
	response_len: number;
	error: string | null;
	system_prompt: string | null;
	prompt_text: string;
	response_text: string;
	max_tokens: number | null;
}

// ── Persistence types ───────────────────────────────────────────────────────

export interface SnapshotCell {
	id: number;
	game_date: string;
	location: string | null;
}

export interface SaveBranchDisplay {
	name: string;
	id: number;
	parent_name: string | null;
	snapshot_count: number;
	latest_location: string | null;
	latest_game_date: string | null;
	snapshots: SnapshotCell[];
}

export interface SaveFileInfo {
	path: string;
	filename: string;
	file_size: string;
	branches: SaveBranchDisplay[];
}

export interface SaveState {
	filename: string | null;
	branch_id: number | null;
	branch_name: string | null;
}
