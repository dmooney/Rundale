import { writable } from 'svelte/store';
import type { ThemePalette } from '$lib/types';
import {
	DEFAULT_THEME_PALETTE,
	SOLARIZED_LIGHT,
	SOLARIZED_DARK,
	applyThemePalette,
	type ThemePreference,
	DEFAULT_PREFERENCE,
	loadThemePreference,
	saveThemePreference
} from '$lib/theme';

/** Game hour < 6 or >= 20 is considered night. */
function isGameNight(hour: number): boolean {
	return hour < 6 || hour >= 20;
}

function createPaletteStore() {
	const { subscribe, set } = writable<ThemePalette>(DEFAULT_THEME_PALETTE);
	let preference: ThemePreference = DEFAULT_PREFERENCE;
	let lastGameHour: number | null = null;

	function apply(p: ThemePalette) {
		set(p);
		applyThemePalette(p);
	}

	function resolveAndApply(pref: ThemePreference) {
		if (pref.name === 'solarized') {
			if (pref.mode === 'auto') {
				// Use the last known game hour if available; default to light
				if (lastGameHour !== null) {
					apply(isGameNight(lastGameHour) ? SOLARIZED_DARK : SOLARIZED_LIGHT);
				} else {
					apply(SOLARIZED_LIGHT);
				}
			} else if (pref.mode === 'dark') {
				apply(SOLARIZED_DARK);
			} else {
				// 'light' or unspecified — default to light
				apply(SOLARIZED_LIGHT);
			}
		}
		// 'default': no-op — server palette is applied via applyServerPalette
	}

	/**
	 * Called by server `"theme-update"` events (time-of-day palette pushes).
	 * Ignored when a user-selected theme is active so the dynamic palette
	 * doesn't overwrite the user's choice.
	 */
	function applyServerPalette(p: ThemePalette) {
		if (preference.name === 'default') apply(p);
	}

	/**
	 * Called on every world-update with the current game hour (0–23).
	 * When solarized auto is active, switches light/dark based on game time.
	 */
	function applyGameHour(hour: number) {
		lastGameHour = hour;
		if (preference.name === 'solarized' && preference.mode === 'auto') {
			apply(isGameNight(hour) ? SOLARIZED_DARK : SOLARIZED_LIGHT);
		}
	}

	/**
	 * Called when a `"theme-switch"` event arrives from the backend
	 * (i.e. the player typed a `/theme` command).
	 */
	function setPreference(pref: ThemePreference) {
		preference = pref;
		saveThemePreference(pref);
		resolveAndApply(pref);
	}

	// ── Initialise from localStorage ─────────────────────────────────────────
	// Restore saved preference immediately so Solarized users never see a flash
	// of the default parchment palette before the server responds.
	const saved = loadThemePreference();
	preference = saved;
	resolveAndApply(saved);

	return { subscribe, applyServerPalette, applyGameHour, setPreference };
}

export const palette = createPaletteStore();
