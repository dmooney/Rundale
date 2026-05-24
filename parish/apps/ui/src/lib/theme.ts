import type { ThemePalette } from './types';

/**
 * Fallback palette used before the backend's `/api/ui-config` response lands
 * (or in tests). Real-life palettes are loaded from the active setting mod's
 * `[theme.palette]` plus any `kind = "asset"` mods under `mods/`.
 */
export const DEFAULT_THEME_PALETTE: ThemePalette = {
	bg: '#fafad8',
	fg: '#31240f',
	accent: '#b08531',
	panel_bg: '#f5f5d3',
	input_bg: '#f0f0ce',
	border: '#cec293',
	muted: '#76663b'
};

/** Wire shape of one theme entry from the backend's registry snapshot. */
export interface ThemeRegistryEntry {
	name: string;
	mode: string;
	label: string;
	palette: ThemePalette;
}

/** Synthetic mode that resolves to another mode based on game time. */
export interface ModeAlias {
	name: string;
	mode: string;
	day_mode: string;
	night_mode: string;
}

/** Wire shape of the full theme registry. */
export interface ThemeRegistrySnapshot {
	themes: ThemeRegistryEntry[];
	mode_aliases: ModeAlias[];
	mode_defaults: Record<string, string>;
}

/** User's persisted theme selection. The set of valid `name`s is dynamic — it
 *  depends on which asset mods are installed. We type it as `string` and let
 *  the registry validate at apply time. */
export interface ThemePreference {
	name: string;
	mode: string;
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
	// When the chat is left-aligned (Zork layout), the "You" label needs to
	// move to the left of its bubble so it doesn't float in the gutter on the
	// right where there's no longer a player bubble anchored to that side.
	setOrClear(
		root,
		'--label-player-text-align',
		palette.chat_align === 'left' ? 'left' : null
	);
	setOrClear(
		root,
		'--label-player-padding-right',
		palette.chat_align === 'left' ? '0' : null
	);
	setOrClear(
		root,
		'--label-player-padding-left',
		palette.chat_align === 'left' ? '0.75rem' : null
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
		// Retro-flat themes (Zork-style): collapse semantic term colors onto
		// the palette so Irish vocabulary / names / locations don't read as
		// off-theme green or yellow inside an otherwise monochrome console.
		root.style.setProperty('--color-irish', palette.fg);
		root.style.setProperty('--color-name', palette.accent);
		root.style.setProperty('--color-location', palette.accent);
		root.style.setProperty('--npc-chips-bg', palette.panel_bg);
	} else {
		root.style.removeProperty('--bubble-npc-bg');
		root.style.removeProperty('--bubble-npc-fg');
		root.style.removeProperty('--bubble-npc-border-left');
		root.style.removeProperty('--bubble-npc-radius');
		root.style.removeProperty('--bubble-player-bg');
		root.style.removeProperty('--bubble-player-fg');
		root.style.removeProperty('--bubble-player-radius');
		root.style.removeProperty('--bubble-font-style');
		root.style.removeProperty('--color-irish');
		root.style.removeProperty('--color-name');
		root.style.removeProperty('--color-location');
		root.style.removeProperty('--npc-chips-bg');
	}

	// Decoration layer: parchment paper-grain + vignette overlays are baked
	// into the rundale base look. Asset-mod themes (Solarized, Zork) opt out
	// via `decoration = "none"` in their `themes.toml` so the palette renders
	// cleanly. Default (unset) preserves the historical parchment overlay.
	setOrClear(root, '--decoration-display', palette.decoration === 'none' ? 'none' : null);

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
