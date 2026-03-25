import { writable } from 'svelte/store';
import type { ThemePalette } from '$lib/types';

const defaultPalette: ThemePalette = {
	bg: '#1a1a2e',
	fg: '#e8e0d0',
	accent: '#c4a35a',
	panel_bg: '#16213e',
	input_bg: '#0f3460',
	border: '#2a2a4a',
	muted: '#7a7a9a'
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
