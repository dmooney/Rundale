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

export interface ThemePreference {
	name: 'default' | 'solarized';
	mode: 'light' | 'dark' | 'auto' | '';
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
}
