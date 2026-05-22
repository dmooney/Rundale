import type { ThemePalette } from './types';

export const DEFAULT_THEME_PALETTE: ThemePalette = {
	bg: '#fafad8',
	fg: '#31240f',
	accent: '#b08531',
	panel_bg: '#f5f5d3',
	input_bg: '#f0f0ce',
	border: '#cec293',
	muted: '#76663b'
};

/** Solarized Light — Ethan Schoonover's palette mapped to Parish color slots. */
export const SOLARIZED_LIGHT: ThemePalette = {
	bg: '#fdf6e3', // base3
	fg: '#586e75', // base00
	accent: '#268bd2', // blue
	panel_bg: '#eee8d5', // base2
	input_bg: '#e6dfc5', // between base2 and base3
	border: '#93a1a1', // base1
	muted: '#93a1a1' // base1
};

/** Solarized Dark — Ethan Schoonover's palette mapped to Parish color slots. */
export const SOLARIZED_DARK: ThemePalette = {
	bg: '#002b36', // base03
	fg: '#839496', // base0
	accent: '#268bd2', // blue
	panel_bg: '#073642', // base02
	input_bg: '#0d3f4f', // slightly lighter than base02
	border: '#586e75', // base01
	muted: '#586e75' // base01
};

const ZORK_FONT = "'Courier New', Consolas, ui-monospace, monospace";

/** Zork on Commodore 64 — VIC-II light-blue on blue, monospace, inverted status bar. */
export const ZORK_C64: ThemePalette = {
	bg: '#1f1b96',
	fg: '#a8a8ff',
	accent: '#ffffff',
	panel_bg: '#1f1b96',
	input_bg: '#15116b',
	border: '#7878ff',
	muted: '#6c5eb5',
	font_body: ZORK_FONT,
	font_display: ZORK_FONT,
	chat_align: 'left',
	bubble_style: 'flat',
	status_invert: true
};

/** Zork on IBM PC / DOS — light-grey on black, monospace, inverted status bar. */
export const ZORK_DOS: ThemePalette = {
	bg: '#000000',
	fg: '#c0c0c0',
	accent: '#ffff55',
	panel_bg: '#000000',
	input_bg: '#0a0a0a',
	border: '#808080',
	muted: '#808080',
	font_body: ZORK_FONT,
	font_display: ZORK_FONT,
	chat_align: 'left',
	bubble_style: 'flat',
	status_invert: true
};

export interface ThemePreference {
	name: 'default' | 'solarized' | 'zork';
	mode: 'light' | 'dark' | 'auto' | 'c64' | 'dos' | '';
}

export const DEFAULT_PREFERENCE: ThemePreference = { name: 'default', mode: '' };

const PREF_KEY = 'parish-theme-preference';

export function loadThemePreference(): ThemePreference {
	// localStorage (not sessionStorage) — deliberate trade-off: theme preference is low-sensitivity UX data; persisting across sessions avoids a flash-of-wrong-theme on reload.
	try {
		const raw = localStorage.getItem(PREF_KEY);
		if (raw) return JSON.parse(raw) as ThemePreference;
	} catch {
		/* ignore corrupt data */
	}
	return DEFAULT_PREFERENCE;
}

export function saveThemePreference(pref: ThemePreference): void {
	// localStorage (not sessionStorage) — deliberate trade-off: theme preference is low-sensitivity UX data; persisting across sessions avoids a flash-of-wrong-theme on reload.
	try {
		localStorage.setItem(PREF_KEY, JSON.stringify(pref));
	} catch {
		/* quota exceeded — ignore */
	}
}

function setOrClear(root: HTMLElement, name: string, value: string | null | undefined): void {
	if (value) root.style.setProperty(name, value);
	else root.style.removeProperty(name);
}

export function applyThemePalette(palette: ThemePalette): void {
	if (typeof document === 'undefined') return;

	const root = document.documentElement;
	root.style.setProperty('--color-bg', palette.bg);
	root.style.setProperty('--color-fg', palette.fg);
	root.style.setProperty('--color-accent', palette.accent);
	root.style.setProperty('--color-panel-bg', palette.panel_bg);
	root.style.setProperty('--color-input-bg', palette.input_bg);
	root.style.setProperty('--color-border', palette.border);
	root.style.setProperty('--color-muted', palette.muted);

	setOrClear(root, '--font-body', palette.font_body);
	setOrClear(root, '--font-display', palette.font_display);

	setOrClear(
		root,
		'--bubble-player-justify',
		palette.chat_align === 'left' ? 'flex-start' : null
	);

	if (palette.bubble_style === 'flat') {
		root.style.setProperty('--bubble-npc-bg', 'transparent');
		root.style.setProperty('--bubble-npc-fg', palette.fg);
		root.style.setProperty('--bubble-npc-border-left', 'none');
		root.style.setProperty('--bubble-npc-radius', '0');
		root.style.setProperty('--bubble-player-bg', 'transparent');
		root.style.setProperty('--bubble-player-fg', palette.fg);
		root.style.setProperty('--bubble-player-radius', '0');
		root.style.setProperty('--bubble-font-style', 'normal');
	} else {
		root.style.removeProperty('--bubble-npc-bg');
		root.style.removeProperty('--bubble-npc-fg');
		root.style.removeProperty('--bubble-npc-border-left');
		root.style.removeProperty('--bubble-npc-radius');
		root.style.removeProperty('--bubble-player-bg');
		root.style.removeProperty('--bubble-player-fg');
		root.style.removeProperty('--bubble-player-radius');
		root.style.removeProperty('--bubble-font-style');
	}

	if (palette.status_invert) {
		// Status bar swaps fg/bg against the chat — all status text becomes palette.bg on palette.fg.
		root.style.setProperty('--status-bg', palette.fg);
		root.style.setProperty('--status-fg', palette.bg);
		root.style.setProperty('--status-accent-fg', palette.bg);
		root.style.setProperty('--status-muted-fg', palette.bg);
		root.style.setProperty('--status-border', palette.bg);
		root.style.setProperty('--status-sep-fg', palette.bg);
		root.style.setProperty('--status-clock-bg', palette.fg);
		root.style.setProperty('--status-clock-fg', palette.bg);
		root.style.setProperty('--status-border-bottom', `2px solid ${palette.fg}`);
	} else {
		root.style.removeProperty('--status-bg');
		root.style.removeProperty('--status-fg');
		root.style.removeProperty('--status-accent-fg');
		root.style.removeProperty('--status-muted-fg');
		root.style.removeProperty('--status-border');
		root.style.removeProperty('--status-sep-fg');
		root.style.removeProperty('--status-clock-bg');
		root.style.removeProperty('--status-clock-fg');
		root.style.removeProperty('--status-border-bottom');
	}
}
