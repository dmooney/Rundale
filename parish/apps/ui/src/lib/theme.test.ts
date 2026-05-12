import { describe, expect, it, beforeEach, vi } from 'vitest';
import { loadThemePreference, saveThemePreference } from './theme';

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
