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
}

export interface MapLocation {
	id: string;
	name: string;
	lat: number;
	lon: number;
	adjacent: boolean;
}

export interface MapData {
	locations: MapLocation[];
	edges: [string, string][];
	player_location: string;
}

export interface NpcInfo {
	name: string;
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

export interface TextLogEntry {
	source: string;
	content: string;
	streaming?: boolean;
}

export interface StreamTokenPayload {
	token: string;
}

export interface StreamEndPayload {
	hints: IrishWordHint[];
}

export interface TextLogPayload {
	source: string;
	content: string;
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
}

export interface ClockDebug {
	game_time: string;
	time_of_day: string;
	season: string;
	festival: string | null;
	weather: string;
	paused: boolean;
	speed_factor: number;
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
	schedule: ScheduleEntryDebug[];
	relationships: RelationshipDebug[];
	memories: MemoryDebug[];
	knowledge: string[];
	intelligence: IntelligenceDebug;
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
}
