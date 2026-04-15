import { writable } from 'svelte/store';
import type { ThemePalette } from '$lib/types';
import {
	DEFAULT_THEME_PALETTE,
	DEFAULT_DARK,
	SOLARIZED_LIGHT,
	SOLARIZED_DARK,
	applyThemePalette,
	setNightClass,
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
	let lastServerPalette: ThemePalette | null = null;
	// True when DEFAULT_DARK is currently showing (via `default dark` or
	// `default auto` at night). Used to block server palette overwrites.
	let nightActive = false;

	function apply(p: ThemePalette) {
		set(p);
		applyThemePalette(p);
		// Identity comparison: DEFAULT_DARK is a module singleton; the server
		// never sends this exact reference.
		nightActive = p === DEFAULT_DARK;
		setNightClass(nightActive);
	}

	function resolveAndApply(pref: ThemePreference) {
		nightActive = false;
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
		} else if (pref.name === 'default') {
			if (pref.mode === 'dark') {
				apply(DEFAULT_DARK);
			} else if (pref.mode === 'light') {
				apply(DEFAULT_THEME_PALETTE);
			} else if (pref.mode === 'auto') {
				if (lastGameHour !== null && isGameNight(lastGameHour)) {
					apply(DEFAULT_DARK);
				} else if (lastServerPalette !== null) {
					apply(lastServerPalette);
				}
				// else: no palette cached yet — wait for first server theme-update
			} else {
				// Empty mode — server palette drives. Clear any lingering night class.
				setNightClass(false);
			}
		}
	}

	/**
	 * Called by server `"theme-update"` events (time-of-day palette pushes).
	 * Ignored when a user-selected theme is active so the dynamic palette
	 * doesn't overwrite the user's choice. Also ignored while the night
	 * variant is showing in `default auto` so the server doesn't overwrite it.
	 */
	function applyServerPalette(p: ThemePalette) {
		lastServerPalette = p;
		if (preference.name !== 'default') return;
		if (nightActive) return;
		apply(p);
	}

	/**
	 * Called on every world-update with the current game hour (0–23).
	 * When solarized auto or default auto is active, switches palette
	 * variants based on game time.
	 */
	function applyGameHour(hour: number) {
		lastGameHour = hour;
		if (preference.name === 'solarized' && preference.mode === 'auto') {
			apply(isGameNight(hour) ? SOLARIZED_DARK : SOLARIZED_LIGHT);
		} else if (preference.name === 'default' && preference.mode === 'auto') {
			const night = isGameNight(hour);
			if (night && !nightActive) {
				apply(DEFAULT_DARK);
			} else if (!night && nightActive) {
				if (lastServerPalette !== null) {
					apply(lastServerPalette);
				} else {
					apply(DEFAULT_THEME_PALETTE);
				}
			}
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
