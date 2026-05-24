import { writable } from 'svelte/store';
import type { ThemePalette } from '$lib/types';
import {
	DEFAULT_THEME_PALETTE,
	applyThemePalette,
	type ThemePreference,
	type ThemeRegistrySnapshot,
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
	// Registry hydrated from `GET /api/ui-config`. Empty until then.
	let registry: ThemeRegistrySnapshot = { themes: [], mode_aliases: [], mode_defaults: {} };
	let modPalette: ThemePalette = DEFAULT_THEME_PALETTE;

	function apply(p: ThemePalette) {
		set(p);
		applyThemePalette(p);
	}

	/** Look up `(name, mode)` in the registry, resolving aliases via game hour. */
	function resolvePalette(name: string, mode: string): ThemePalette | null {
		if (name === 'default' || name === '') return modPalette;

		// Synthetic mode? (e.g. solarized auto)
		const alias = registry.mode_aliases.find((a) => a.name === name && a.mode === mode);
		if (alias) {
			const isNight = lastGameHour !== null ? isGameNight(lastGameHour) : false;
			const resolvedMode = isNight ? alias.night_mode : alias.day_mode;
			const entry = registry.themes.find((t) => t.name === name && t.mode === resolvedMode);
			return entry?.palette ?? null;
		}

		// Direct hit
		const entry = registry.themes.find((t) => t.name === name && t.mode === mode);
		if (entry) return entry.palette;

		// Name with no explicit mode → consult mode_defaults
		if (mode === '') {
			const defaultMode = registry.mode_defaults[name];
			if (defaultMode) return resolvePalette(name, defaultMode);
		}

		return null;
	}

	function resolveAndApply(pref: ThemePreference) {
		const palette = resolvePalette(pref.name, pref.mode);
		if (palette) {
			apply(palette);
		}
		// Unknown theme: leave current palette in place. The backend's
		// `theme-switch` event will deliver a palette next time the user
		// types `/theme <name>`.
	}

	/**
	 * Called by server `"theme-update"` events (time-of-day palette pushes).
	 * Ignored when a user-selected theme is active so the dynamic palette
	 * doesn't overwrite the user's choice.
	 */
	function applyServerPalette(p: ThemePalette) {
		if (preference.name === 'default' || preference.name === '') apply(p);
	}

	/**
	 * Called on every world-update with the current game hour (0–23).
	 * When a synthetic-alias mode is active (e.g. solarized auto), switches
	 * the resolved palette as we cross day/night.
	 */
	function applyGameHour(hour: number) {
		const previousHour = lastGameHour;
		lastGameHour = hour;
		const alias = registry.mode_aliases.find(
			(a) => a.name === preference.name && a.mode === preference.mode
		);
		if (!alias) return;
		const wasNight = previousHour !== null ? isGameNight(previousHour) : false;
		const isNight = isGameNight(hour);
		if (wasNight !== isNight) {
			const resolved = isNight ? alias.night_mode : alias.day_mode;
			const entry = registry.themes.find((t) => t.name === alias.name && t.mode === resolved);
			if (entry) apply(entry.palette);
		}
	}

	/**
	 * Called when a `"theme-switch"` event arrives from the backend
	 * (i.e. the player typed a `/theme` command). The event carries the
	 * resolved palette directly so we don't have to look it up.
	 */
	function setPreference(pref: ThemePreference, palette?: ThemePalette | null) {
		preference = pref;
		saveThemePreference(pref);
		if (palette) {
			apply(palette);
		} else {
			resolveAndApply(pref);
		}
	}

	/**
	 * Hydrate the registry and the setting-mod's default palette from
	 * `GET /api/ui-config`. Called once at startup.
	 */
	function hydrateFromUiConfig(snapshot: ThemeRegistrySnapshot, defaultPalette: ThemePalette) {
		registry = snapshot;
		modPalette = defaultPalette;
		// Re-resolve only a user-selected (non-default) theme now that the
		// registry is loaded — fixes the case where the user had `/theme zork`
		// saved but the fallback applied at boot.
		//
		// For the default preference we deliberately do NOT re-apply
		// `defaultPalette`: the server pushes a live time-of-day palette
		// (interpolated from the mod's keyframes) via `theme-update`, and the
		// static `[theme.palette]` snapshot would overwrite it — causing a colour
		// jump at boot and, on the web runtime (which sends no `theme-update`),
		// stranding the UI on the wrong palette.
		if (preference.name !== 'default' && preference.name !== '') {
			resolveAndApply(preference);
		}
	}

	// ── Initialise from localStorage ─────────────────────────────────────────
	// Restore saved preference immediately so users never see a flash of the
	// default palette before the registry hydrates. Resolution against the
	// (still-empty) registry is a no-op for any non-default name; once
	// hydrateFromUiConfig() runs, the real palette is applied.
	const saved = loadThemePreference();
	preference = saved;
	resolveAndApply(saved);

	return {
		subscribe,
		applyServerPalette,
		applyGameHour,
		setPreference,
		hydrateFromUiConfig
	};
}

export const palette = createPaletteStore();
