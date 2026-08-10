/// Mock data for Playwright E2E tests.
/// These mirror the Rust types serialized via serde (snake_case).

import type {
	WorldSnapshot,
	MapData,
	NpcInfo,
	ThemePalette,
	LanguageHint,
	TextLogEntry,
	UiConfig,
	DebugSnapshot,
	SaveFileInfo,
	SaveState,
} from '../src/lib/types';
import type { SetupSnapshot } from '../src/lib/ipc';
import type { ModSummary, EditorModSnapshot } from '../src/lib/editor-types';
import { DEFAULT_THEME_PALETTE } from '../src/lib/theme';

// ── Theme palettes used in tests ────────────────────────────────────────────

export const PALETTES: Record<string, ThemePalette> = {
	default: DEFAULT_THEME_PALETTE,
	morning: {
		bg: '#1e2a3a',
		fg: '#f0e6d2',
		accent: '#d4a44a',
		panel_bg: '#1a2438',
		input_bg: '#162040',
		border: '#2e3e56',
		muted: '#8899aa',
	},
	midday: {
		bg: '#2a2a3e',
		fg: '#f5eed8',
		accent: '#c4a35a',
		panel_bg: '#222240',
		input_bg: '#1a1a50',
		border: '#3a3a5a',
		muted: '#8888aa',
	},
	dusk: {
		bg: '#2a1a2e',
		fg: '#e8d8c8',
		accent: '#d47a3a',
		panel_bg: '#241828',
		input_bg: '#301040',
		border: '#3e2a4a',
		muted: '#9a7a8a',
	},
	night: {
		bg: '#0e0e1e',
		fg: '#c8c0b0',
		accent: '#6a7aaa',
		panel_bg: '#0a0a18',
		input_bg: '#080830',
		border: '#1a1a3a',
		muted: '#5a5a7a',
	},
};

// ── World snapshots per time of day ─────────────────────────────────────────

/**
 * Build a UTC epoch for a given hour (today's date, UTC).
 * StatusBar derives display time from game_epoch_ms via requestAnimationFrame,
 * so this must encode the correct hour.
 */
function epochForHour(hour: number): number {
	const d = new Date();
	d.setUTCHours(hour, 0, 0, 0);
	return d.getTime();
}

function makeSnapshot(
	hour: number,
	timeLabel: string,
	weather: string = 'Overcast',
): WorldSnapshot {
	return {
		// Dublin is deliberately not one of Rundale's three authored plates,
		// exercising the neutral uncovered-location scene in generic E2E tests.
		location_id: 404,
		location_name: 'Baile Átha Cliath',
		location_description:
			'The streets of Dublin bustle with life. Georgian buildings line the wide avenues, and the Liffey flows dark beneath its bridges.',
		time_label: timeLabel,
		hour,
		minute: 0,
		weather,
		season: 'Spring',
		festival: null,
		paused: false,
		inference_paused: false,
		game_epoch_ms: epochForHour(hour),
		speed_factor: 0, // Frozen: clock stays at the anchored hour during tests
		name_hints: [
			{
				word: 'Baile Átha Cliath',
				pronunciation: 'BAHL-ya AH-ha KLEE-ah',
				meaning: 'town of the hurdled ford (Dublin)',
			},
			{ word: 'Aoife', pronunciation: 'EE-fa', meaning: 'beauty, radiance' },
		],
		day_of_week: 'Monday',
		active_tasks: [],
	};
}

export const SNAPSHOTS: Record<string, WorldSnapshot> = {
	morning: makeSnapshot(8, 'Morning', 'Clear'),
	midday: makeSnapshot(12, 'Midday', 'Overcast'),
	dusk: makeSnapshot(18, 'Dusk', 'Drizzle'),
	night: makeSnapshot(22, 'Night', 'Clear'),
};

// ── Map data ────────────────────────────────────────────────────────────────

export const MAP_DATA: MapData = {
	locations: [
		{
			id: 'dublin',
			name: 'Baile Átha Cliath',
			lat: 53.3498,
			lon: -6.2603,
			adjacent: false,
			hops: 0,
		},
		{
			id: 'howth',
			name: 'Binn Éadair',
			lat: 53.3862,
			lon: -6.065,
			adjacent: true,
			hops: 1,
		},
		{
			id: 'dalkey',
			name: 'Deilginse',
			lat: 53.2758,
			lon: -6.0986,
			adjacent: true,
			hops: 1,
		},
		{
			id: 'bray',
			name: 'Bré',
			lat: 53.2009,
			lon: -6.0985,
			adjacent: false,
			hops: 2,
		},
		{
			id: 'maynooth',
			name: 'Maigh Nuad',
			lat: 53.3851,
			lon: -6.5916,
			adjacent: false,
			hops: 1,
		},
	],
	edges: [
		['dublin', 'howth'],
		['dublin', 'dalkey'],
		['dalkey', 'bray'],
		['dublin', 'maynooth'],
	],
	player_location: 'dublin',
	player_lat: 53.3498,
	player_lon: -6.2603,
	transport_label: 'on foot',
	transport_id: 'walking',
};

// ── NPCs ────────────────────────────────────────────────────────────────────

export const NPCS: NpcInfo[] = [
	{
		name: 'Séamas Ó Briain',
		real_name: 'Séamas Ó Briain',
		occupation: 'Publican',
		mood: 'cheerful',
		introduced: true,
		mood_emoji: '😊',
	},
	{
		name: 'Aoife Ní Cheallaigh',
		real_name: 'Aoife Ní Cheallaigh',
		occupation: 'Scholar',
		mood: 'pensive',
		introduced: true,
		mood_emoji: '🤔',
	},
];

// ── Irish word hints ────────────────────────────────────────────────────────

export const IRISH_HINTS: LanguageHint[] = [
	{ word: 'sláinte', pronunciation: 'SLAWN-cha', meaning: 'health / cheers' },
	{ word: 'craic', pronunciation: 'crack', meaning: 'fun, entertainment' },
];

// ── UI config ──────────────────────────────────────────────────────────────

export const UI_CONFIG: UiConfig = {
	hints_label: 'Focail (Irish Words)',
	default_accent: DEFAULT_THEME_PALETTE.accent,
	splash_text: '',
	active_tile_source: 'osm',
	tile_sources: [
		{
			id: 'osm',
			label: 'OpenStreetMap',
			url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
			tile_size: 256,
			minzoom: 0,
			maxzoom: 19,
			attribution: '© OpenStreetMap contributors',
			raster_saturation: 0,
			raster_opacity: 1,
			tms: false,
		},
	],
	auto_pause_timeout_seconds: 300,
};

// ── Text log entries ────────────────────────────────────────────────────────

export const TEXT_LOG: TextLogEntry[] = [
	{
		source: 'system',
		content:
			'The streets of Dublin bustle with life. Georgian buildings line the wide avenues, and the Liffey flows dark beneath its bridges.',
	},
	{
		source: 'player',
		content: 'talk to Séamas',
	},
	{
		source: 'NPC',
		content:
			"Ah, you're most welcome! Come in out of the rain. What'll it be — a pint of the black stuff, or something warmer?",
	},
];

// ── Debug snapshot ──────────────────────────────────────────────────────────

export const DEBUG_SNAPSHOT: DebugSnapshot = {
	clock: {
		game_time: '08:00',
		time_of_day: 'Morning',
		season: 'Spring',
		festival: null,
		weather: 'Clear',
		paused: false,
		inference_paused: false,
		speed_factor: 0,
		speed_name: null,
		day_of_week: 'Monday',
		day_type: 'weekday',
		start_game_time: '08:00',
		paused_game_time: '',
		real_elapsed_secs: 0,
	},
	weather: {
		current: 'Clear',
		since: '00:00',
		duration_hours: 6,
		min_duration_hours: 2,
		last_check_hour: 8,
	},
	world: {
		player_location_name: 'Baile Átha Cliath',
		player_location_id: 1,
		location_count: 5,
		visited_count: 1,
		visited_locations: ['Baile Átha Cliath'],
		edge_traversals: [],
		text_log_tail: [],
		text_log_len: 0,
		locations: [],
		player_name: null,
	},
	npcs: [],
	tier_summary: {
		tier1_count: 0,
		tier2_count: 0,
		tier3_count: 0,
		tier4_count: 0,
		tier1_names: [],
		tier2_names: [],
		tier3_names: [],
		tier4_names: [],
		tier3_in_flight: false,
		last_tier2_tick: null,
		last_tier3_tick: null,
		last_tier4_tick: null,
		introduced_count: 0,
		tier2_in_flight: false,
		tier3_pending_count: 0,
		tier4_recent_events: [],
	},
	event_bus: {
		subscriber_count: 0,
		recent_events: [],
	},
	gossip: {
		item_count: 0,
		items: [],
	},
	conversations: {
		exchange_count: 0,
		exchanges: [],
	},
	events: [],
	inference: {
		provider_name: 'google',
		model_name: 'gemini-3.6-flash',
		base_url: 'https://generativelanguage.googleapis.com/v1',
		cloud_provider: null,
		cloud_model: null,
		has_queue: false,
		reaction_req_id: 0,
		improv_enabled: false,
		tier2_parse_failures_total: 0,
		call_log: [
			{
				request_id: 42,
				timestamp: '09:36',
				model: 'gemini-3.6-flash',
				provider: 'google',
				api_mode: 'google-interactions-v1',
				role: 'dialogue',
				subrole: 'dialogue',
				streaming: true,
				duration_ms: 420,
				prompt_len: 9000,
				response_len: 40,
				error: null,
				system_prompt: null,
				prompt_text: '',
				response_text: '',
				max_tokens: 4096,
				ttft_ms: 120,
				output_tokens: 10,
				stream_chunks: 4,
				input_tokens: 9000,
				cached_tokens: 8000,
				thought_tokens: 20,
				total_tokens: 9030,
				thinking_level: 'minimal',
				requested_service_tier: 'standard',
				effective_service_tier: 'standard',
				provider_request_id: 'int_test',
				terminal_status: 'completed',
				retry_count: 0,
				http_status: 200,
				failure_kind: null,
				partial_output_len: 0,
				tier_downgraded: false,
				estimated_cost_usd: 0.002,
				prompt_prefix_hash: 'abcd',
				prompt_prefix_len: 8500,
			},
		],
		categories: [
			{
				role: 'dialogue',
				provider: 'google',
				model: 'gemini-3.6-flash',
				base_url: 'https://generativelanguage.googleapis.com/v1',
				thinking_level: 'minimal',
				max_output_tokens: 4096,
				service_tier: 'standard',
			},
		],
		configured_providers: ['google'],
	},
	auth: {
		oauth_enabled: false,
		logged_in: false,
		provider: null,
		display_name: null,
		session_id: null,
	},
};

// ── Setup snapshot (completed — prevents overlay from blocking UI) ───────────

export const SETUP_SNAPSHOT: SetupSnapshot = {
	current_message: '',
	messages: [],
	completed: 0,
	total: 0,
	done: true,
	success: true,
	error: '',
};

// ── Save files ──────────────────────────────────────────────────────────────

export const SAVE_FILES: SaveFileInfo[] = [
	{
		path: '/saves/rundale.ledger',
		filename: 'rundale.ledger',
		file_size: '1.2 MB',
		branches: [
			{
				name: 'main',
				id: 1,
				parent_name: null,
				snapshot_count: 3,
				latest_location: 'Baile Átha Cliath',
				latest_game_date: '1820-03-15',
				snapshots: [
					{ id: 1, game_date: '1820-03-15', location: 'Baile Átha Cliath' },
					{ id: 2, game_date: '1820-03-16', location: 'Howth' },
					{ id: 3, game_date: '1820-03-17', location: 'Baile Átha Cliath' },
				],
			},
		],
		locked: false,
	},
];

export const SAVE_STATE: SaveState = {
	filename: 'rundale.ledger',
	branch_id: 1,
	branch_name: 'main',
};

// ── Editor (Parish Designer) ────────────────────────────────────────────────

export const EDITOR_MODS: ModSummary[] = [
	{
		id: 'rundale',
		name: 'rundale',
		title: 'Rundale',
		version: '0.1.0',
		description: 'Test mod for e2e',
		path: '/mods/rundale',
	},
];

export const EDITOR_SNAPSHOT: EditorModSnapshot = {
	mod_path: '/mods/rundale',
	manifest: {
		id: 'rundale',
		name: 'rundale',
		title: 'Rundale',
		version: '0.1.0',
		description: 'Test mod for e2e',
		start_date: '1820-03-15',
		start_location: 1,
		period_year: 1820,
	},
	npcs: { npcs: [] },
	locations: [],
	festivals: [],
	encounters: {},
	anachronisms: {
		context_alert_prefix: '',
		context_alert_suffix: '',
		terms: [],
	},
	validation: { errors: [], warnings: [] },
};
