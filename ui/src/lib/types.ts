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

export interface IrishWordHint {
	word: string;
	pronunciation: string;
	meaning: string | null;
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
}
