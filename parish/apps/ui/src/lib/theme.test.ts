import { describe, expect, it, beforeEach, vi } from 'vitest';
import {
	loadThemePreference,
	saveThemePreference,
	applyThemePalette,
	DEFAULT_THEME_PALETTE,
	SOLARIZED_LIGHT,
	THEME_TRANSITION_MS,
} from './theme';

describe('loadThemePreference / saveThemePreference', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('returns the default preference when nothing is stored', () => {
		const pref = loadThemePreference();
		expect(pref.name).toBe('default');
		expect(pref.mode).toBe('');
	});

	it('round-trips a saved preference', () => {
		saveThemePreference({ name: 'solarized', mode: 'dark' });
		const loaded = loadThemePreference();
		expect(loaded.name).toBe('solarized');
		expect(loaded.mode).toBe('dark');
	});

	it('returns default on corrupt JSON in localStorage', () => {
		localStorage.setItem('parish-theme-preference', '{invalid json}');
		const pref = loadThemePreference();
		expect(pref.name).toBe('default');
		expect(pref.mode).toBe('');
	});

	it('does not throw on save when localStorage is full', () => {
		// Simulate quota exceeded — jsdom doesn't throw, but the catch clause
		// should swallow it gracefully. We verify by checking no exception.
		const setItem = vi
			.spyOn(Storage.prototype, 'setItem')
			.mockImplementation(() => {
				throw new DOMException('QuotaExceededError');
			});
		expect(() =>
			saveThemePreference({ name: 'solarized', mode: 'light' }),
		).not.toThrow();
		setItem.mockRestore();
	});
});

describe('applyThemePalette theme-transition scoping', () => {
	it('adds the theme-transition class during a palette swap and removes it after', async () => {
		vi.useFakeTimers();
		try {
			applyThemePalette(DEFAULT_THEME_PALETTE);
			expect(
				document.documentElement.classList.contains('theme-transition'),
			).toBe(true);
			vi.advanceTimersByTime(THEME_TRANSITION_MS + 50);
			expect(
				document.documentElement.classList.contains('theme-transition'),
			).toBe(false);
		} finally {
			vi.useRealTimers();
		}
	});

	it('debounces back-to-back swaps into one removal timer', () => {
		vi.useFakeTimers();
		try {
			applyThemePalette(DEFAULT_THEME_PALETTE);
			vi.advanceTimersByTime(THEME_TRANSITION_MS / 2);
			applyThemePalette(SOLARIZED_LIGHT);
			// The first timer would have fired by now without the reset.
			vi.advanceTimersByTime(THEME_TRANSITION_MS / 2 + 50);
			expect(
				document.documentElement.classList.contains('theme-transition'),
			).toBe(true);
			vi.advanceTimersByTime(THEME_TRANSITION_MS);
			expect(
				document.documentElement.classList.contains('theme-transition'),
			).toBe(false);
		} finally {
			vi.useRealTimers();
		}
	});
});
