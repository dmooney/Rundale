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
import {
	getSunTimes,
	isNightNow,
	nextSwitchTime,
	getUserCoords,
	IRELAND_LAT,
	IRELAND_LON
} from '$lib/sun';

function createPaletteStore() {
	const { subscribe, set } = writable<ThemePalette>(DEFAULT_THEME_PALETTE);
	let preference: ThemePreference = DEFAULT_PREFERENCE;
	let switchTimer: ReturnType<typeof setTimeout> | null = null;
	let coords = { lat: IRELAND_LAT, lon: IRELAND_LON };

	function apply(p: ThemePalette) {
		set(p);
		applyThemePalette(p);
	}

	function resolveAndApply(pref: ThemePreference) {
		if (pref.name === 'solarized') {
			if (pref.mode === 'auto') {
				const times = getSunTimes(coords.lat, coords.lon, new Date());
				apply(isNightNow(times.sunrise, times.sunset) ? SOLARIZED_DARK : SOLARIZED_LIGHT);
				scheduleNext(times);
			} else if (pref.mode === 'dark') {
				apply(SOLARIZED_DARK);
			} else {
				// 'light' or unspecified — default to light
				apply(SOLARIZED_LIGHT);
			}
		}
		// 'default': no-op — server palette is applied via applyServerPalette
	}

	function scheduleNext(times: { sunrise: Date; sunset: Date }) {
		if (switchTimer !== null) {
			clearTimeout(switchTimer);
			switchTimer = null;
		}
		const next = nextSwitchTime(times.sunrise, times.sunset);
		if (!next) return;
		const ms = next.getTime() - Date.now();
		if (ms <= 0) return;
		// +1 s buffer so we're safely past the threshold when we re-evaluate
		switchTimer = setTimeout(() => {
			switchTimer = null;
			const today = new Date();
			const newTimes = getSunTimes(coords.lat, coords.lon, today);
			resolveAndApply(preference);
			// resolveAndApply re-schedules the next switch via scheduleNext
		}, ms + 1000);
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
	 * Called when a `"theme-switch"` event arrives from the backend
	 * (i.e. the player typed a `/theme` command).
	 */
	async function setPreference(pref: ThemePreference) {
		preference = pref;
		saveThemePreference(pref);
		if (pref.mode === 'auto') {
			// Refresh geolocation; fall back to Ireland on failure
			try {
				coords = await getUserCoords();
			} catch {
				/* keep current coords */
			}
		}
		resolveAndApply(pref);
	}

	// ── Initialise from localStorage ─────────────────────────────────────────
	// Restore saved preference immediately so Solarized users never see a flash
	// of the default parchment palette before the server responds.
	const saved = loadThemePreference();
	preference = saved;
	resolveAndApply(saved);

	// If auto mode was saved, kick off an async geolocation fetch; if the
	// resolved coords differ from Ireland, re-apply so the schedule is accurate.
	if (saved.mode === 'auto') {
		getUserCoords()
			.then((c) => {
				if (c.lat !== coords.lat || c.lon !== coords.lon) {
					coords = c;
					resolveAndApply(preference);
				}
			})
			.catch(() => {});
	}

	return { subscribe, applyServerPalette, setPreference };
}

export const palette = createPaletteStore();
