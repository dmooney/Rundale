import { describe, it, expect, beforeEach } from 'vitest';
import { applyThemePalette, DEFAULT_THEME_PALETTE } from './theme';
import type { ThemePalette } from './types';

const PARCHMENT: ThemePalette = DEFAULT_THEME_PALETTE;

const ZORK_C64: ThemePalette = {
	bg: '#1f1b96',
	fg: '#a8a8ff',
	accent: '#ffffff',
	panel_bg: '#1f1b96',
	input_bg: '#15116b',
	border: '#7878ff',
	muted: '#6c5eb5',
	font_body: "'Courier New', monospace",
	font_display: "'Courier New', monospace",
	chat_align: 'left',
	bubble_style: 'flat',
	status_invert: true
};

describe('applyThemePalette', () => {
	beforeEach(() => {
		document.documentElement.removeAttribute('style');
	});

	it('writes the seven base color slots for any palette', () => {
		applyThemePalette(PARCHMENT);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--color-bg')).toBe(PARCHMENT.bg);
		expect(s.getPropertyValue('--color-fg')).toBe(PARCHMENT.fg);
		expect(s.getPropertyValue('--color-accent')).toBe(PARCHMENT.accent);
	});

	it('does not set structural overrides for a palette without them', () => {
		applyThemePalette(PARCHMENT);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--bubble-player-justify')).toBe('');
		expect(s.getPropertyValue('--status-bg')).toBe('');
		expect(s.getPropertyValue('--bubble-npc-bg')).toBe('');
	});

	it('applies monospace font, left chat alignment, flat bubbles, and inverted status when set', () => {
		applyThemePalette(ZORK_C64);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--font-body')).toContain('monospace');
		expect(s.getPropertyValue('--font-display')).toContain('monospace');
		expect(s.getPropertyValue('--bubble-player-justify')).toBe('flex-start');
		expect(s.getPropertyValue('--bubble-npc-bg')).toBe('transparent');
		expect(s.getPropertyValue('--bubble-player-bg')).toBe('transparent');
		expect(s.getPropertyValue('--bubble-font-style')).toBe('normal');
		// Inverted status: bg becomes palette.fg, fg becomes palette.bg
		expect(s.getPropertyValue('--status-bg')).toBe(ZORK_C64.fg);
		expect(s.getPropertyValue('--status-fg')).toBe(ZORK_C64.bg);
		expect(s.getPropertyValue('--status-accent-fg')).toBe(ZORK_C64.bg);
	});

	it('clears all structural overrides when switching back to a plain palette', () => {
		applyThemePalette(ZORK_C64);
		applyThemePalette(PARCHMENT);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--bubble-player-justify')).toBe('');
		expect(s.getPropertyValue('--bubble-npc-bg')).toBe('');
		expect(s.getPropertyValue('--status-bg')).toBe('');
		expect(s.getPropertyValue('--font-body')).toBe('');
	});
});
