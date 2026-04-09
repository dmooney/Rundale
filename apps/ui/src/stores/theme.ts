import { writable } from 'svelte/store';
import type { ThemePalette } from '$lib/types';
import { DEFAULT_THEME_PALETTE, applyThemePalette } from '$lib/theme';

function createPaletteStore() {
	const { subscribe, set } = writable<ThemePalette>(DEFAULT_THEME_PALETTE);

	function apply(palette: ThemePalette) {
		set(palette);
		applyThemePalette(palette);
	}

	// Apply defaults immediately
	apply(DEFAULT_THEME_PALETTE);

	return { subscribe, apply };
}

export const palette = createPaletteStore();
