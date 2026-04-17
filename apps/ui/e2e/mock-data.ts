/// Mock data for Playwright E2E tests.
/// These mirror the Rust types serialized via serde (snake_case).

import type {
	WorldSnapshot,
	MapData,
	NpcInfo,
	ThemePalette,
	LanguageHint,
	TextLogEntry,
	UiConfig
} from '../src/lib/types';
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
		muted: '#8899aa'
	},
	midday: {
		bg: '#2a2a3e',
		fg: '#f5eed8',
		accent: '#c4a35a',
		panel_bg: '#222240',
		input_bg: '#1a1a50',
		border: '#3a3a5a',
		muted: '#8888aa'
	},
	dusk: {
		bg: '#2a1a2e',
		fg: '#e8d8c8',
		accent: '#d47a3a',
		panel_bg: '#241828',
		input_bg: '#301040',
		border: '#3e2a4a',
		muted: '#9a7a8a'
	},
	night: {
		bg: '#0e0e1e',
		fg: '#c8c0b0',
		accent: '#6a7aaa',
		panel_bg: '#0a0a18',
		input_bg: '#080830',
		border: '#1a1a3a',
		muted: '#5a5a7a'
	}
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
	weather: string = 'Overcast'
): WorldSnapshot {
	return {
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
			{ word: 'Baile Átha Cliath', pronunciation: 'BAHL-ya AH-ha KLEE-ah', meaning: 'town of the hurdled ford (Dublin)' },
			{ word: 'Aoife', pronunciation: 'EE-fa', meaning: 'beauty, radiance' }
		],
		day_of_week: 'Monday'
	};
}

export const SNAPSHOTS: Record<string, WorldSnapshot> = {
	morning: makeSnapshot(8, 'Morning', 'Clear'),
	midday: makeSnapshot(12, 'Midday', 'Overcast'),
	dusk: makeSnapshot(18, 'Dusk', 'Drizzle'),
	night: makeSnapshot(22, 'Night', 'Clear')
};

// ── Map data ────────────────────────────────────────────────────────────────

export const MAP_DATA: MapData = {
	locations: [
		{ id: 'dublin', name: 'Baile Átha Cliath', lat: 53.3498, lon: -6.2603, adjacent: false, hops: 0 },
		{ id: 'howth', name: 'Binn Éadair', lat: 53.3862, lon: -6.065, adjacent: true, hops: 1 },
		{ id: 'dalkey', name: 'Deilginse', lat: 53.2758, lon: -6.0986, adjacent: true, hops: 1 },
		{ id: 'bray', name: 'Bré', lat: 53.2009, lon: -6.0985, adjacent: false, hops: 2 },
		{ id: 'maynooth', name: 'Maigh Nuad', lat: 53.3851, lon: -6.5916, adjacent: false, hops: 1 }
	],
	edges: [
		['dublin', 'howth'],
		['dublin', 'dalkey'],
		['dalkey', 'bray'],
		['dublin', 'maynooth']
	],
	player_location: 'dublin',
	player_lat: 53.3498,
	player_lon: -6.2603
};

// ── NPCs ────────────────────────────────────────────────────────────────────

export const NPCS: NpcInfo[] = [
	{ name: 'Séamas Ó Briain', real_name: 'Séamas Ó Briain', occupation: 'Publican', mood: 'cheerful', introduced: true, mood_emoji: '😊' },
	{ name: 'Aoife Ní Cheallaigh', real_name: 'Aoife Ní Cheallaigh', occupation: 'Scholar', mood: 'pensive', introduced: true, mood_emoji: '🤔' }
];

// ── Irish word hints ────────────────────────────────────────────────────────

export const IRISH_HINTS: LanguageHint[] = [
	{ word: 'sláinte', pronunciation: 'SLAWN-cha', meaning: 'health / cheers' },
	{ word: 'craic', pronunciation: 'crack', meaning: 'fun, entertainment' }
];

// ── UI config ──────────────────────────────────────────────────────────────

export const UI_CONFIG: UiConfig = {
	hints_label: 'Focail (Irish Words)',
	default_accent: DEFAULT_THEME_PALETTE.accent,
	splash_text: ''
};

// ── Text log entries ────────────────────────────────────────────────────────

export const TEXT_LOG: TextLogEntry[] = [
	{
		source: 'system',
		content:
			'The streets of Dublin bustle with life. Georgian buildings line the wide avenues, and the Liffey flows dark beneath its bridges.'
	},
	{
		source: 'player',
		content: 'talk to Séamas'
	},
	{
		source: 'NPC',
		content:
			"Ah, you're most welcome! Come in out of the rain. What'll it be — a pint of the black stuff, or something warmer?"
	}
];
