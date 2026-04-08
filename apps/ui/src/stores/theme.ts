import { writable } from 'svelte/store';
import type { ThemePalette } from '$lib/types';

const defaultPalette: ThemePalette = {
	bg: '#fff5dc',
	fg: '#32230f',
	accent: '#b48232',
	panel_bg: '#faf0d7',
	input_bg: '#f5ebd2',
	border: '#d2be96',
	muted: '#78643c'
};

function createPaletteStore() {
	const { subscribe, set } = writable<ThemePalette>(defaultPalette);

	function apply(palette: ThemePalette) {
		set(palette);
		if (typeof document !== 'undefined') {
			const root = document.documentElement;
			root.style.setProperty('--color-bg', palette.bg);
			root.style.setProperty('--color-fg', palette.fg);
			root.style.setProperty('--color-accent', palette.accent);
			root.style.setProperty('--color-panel-bg', palette.panel_bg);
			root.style.setProperty('--color-input-bg', palette.input_bg);
			root.style.setProperty('--color-border', palette.border);
			root.style.setProperty('--color-muted', palette.muted);
		}
	}

	// Apply defaults immediately
	apply(defaultPalette);

	return { subscribe, apply };
}

export const palette = createPaletteStore();
