import { beforeEach, describe, expect, it } from 'vitest';
import { HISTORY_KEY, HISTORY_MAX, loadHistory, saveHistory } from './history';

describe('loadHistory', () => {
	beforeEach(() => {
		sessionStorage.clear();
	});

	it('returns empty array when nothing stored', () => {
		expect(loadHistory()).toEqual([]);
	});

	it('returns empty array when stored data is corrupt', () => {
		sessionStorage.setItem(HISTORY_KEY, 'not-valid-json{{{');
		expect(loadHistory()).toEqual([]);
	});
});

describe('saveHistory / loadHistory', () => {
	beforeEach(() => {
		sessionStorage.clear();
	});

	it('round-trips a list of entries', () => {
		const entries = ['look', 'go north', 'talk to Brigid'];
		saveHistory(entries);
		expect(loadHistory()).toEqual(entries);
	});

	it('overwrites previous value', () => {
		saveHistory(['old entry']);
		saveHistory(['new entry']);
		expect(loadHistory()).toEqual(['new entry']);
	});
});

describe('constants', () => {
	it('HISTORY_KEY is a non-empty string', () => {
		expect(typeof HISTORY_KEY).toBe('string');
		expect(HISTORY_KEY.length).toBeGreaterThan(0);
	});

	it('HISTORY_MAX is a positive integer', () => {
		expect(Number.isInteger(HISTORY_MAX)).toBe(true);
		expect(HISTORY_MAX).toBeGreaterThan(0);
	});
});
