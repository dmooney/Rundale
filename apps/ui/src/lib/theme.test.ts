import { describe, it, expect, beforeEach } from 'vitest';
import {
	applyThemePalette,
	DEFAULT_THEME_PALETTE,
	SOLARIZED_LIGHT,
	ZORK_C64,
	ZORK_DOS
} from './theme';

describe('applyThemePalette', () => {
	beforeEach(() => {
		document.documentElement.removeAttribute('style');
	});

	it('writes the seven base color slots for any palette', () => {
		applyThemePalette(SOLARIZED_LIGHT);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--color-bg')).toBe(SOLARIZED_LIGHT.bg);
		expect(s.getPropertyValue('--color-fg')).toBe(SOLARIZED_LIGHT.fg);
		expect(s.getPropertyValue('--color-accent')).toBe(SOLARIZED_LIGHT.accent);
	});

	it('does not set structural overrides for a palette without them', () => {
		applyThemePalette(SOLARIZED_LIGHT);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--bubble-player-justify')).toBe('');
		expect(s.getPropertyValue('--status-bg')).toBe('');
		expect(s.getPropertyValue('--bubble-npc-bg')).toBe('');
	});

	it('sets monospace font, left chat alignment, flat bubbles, and inverted status for Zork C64', () => {
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

	it('uses black/grey for Zork DOS', () => {
		applyThemePalette(ZORK_DOS);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--color-bg')).toBe('#000000');
		expect(s.getPropertyValue('--color-fg')).toBe('#c0c0c0');
		expect(s.getPropertyValue('--status-bg')).toBe('#c0c0c0');
		expect(s.getPropertyValue('--status-fg')).toBe('#000000');
	});

	it('clears Zork overrides when switching back to a plain palette', () => {
		applyThemePalette(ZORK_C64);
		applyThemePalette(DEFAULT_THEME_PALETTE);
		const s = document.documentElement.style;
		expect(s.getPropertyValue('--bubble-player-justify')).toBe('');
		expect(s.getPropertyValue('--bubble-npc-bg')).toBe('');
		expect(s.getPropertyValue('--status-bg')).toBe('');
		expect(s.getPropertyValue('--font-body')).toBe('');
	});
});
