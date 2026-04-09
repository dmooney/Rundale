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
