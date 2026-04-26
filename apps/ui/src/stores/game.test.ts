import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { textLog, pushErrorLog, formatIpcError, loadingColor, focailOpen } from './game';

describe('pushErrorLog', () => {
	beforeEach(() => {
		textLog.set([]);
	});

	it('appends a system entry with the error subtype', () => {
		pushErrorLog('Something went wrong');
		const log = get(textLog);
		expect(log.length).toBe(1);
		expect(log[0]).toMatchObject({
			source: 'system',
			subtype: 'error',
			content: 'Something went wrong'
		});
	});

	it('appends to existing log entries rather than replacing them', () => {
		textLog.set([{ source: 'system', content: 'Welcome.' }]);
		pushErrorLog('Network down');
		const log = get(textLog);
		expect(log.length).toBe(2);
		expect(log[0].content).toBe('Welcome.');
		expect(log[1].subtype).toBe('error');
	});
});

describe('loadingColor', () => {
	beforeEach(() => {
		loadingColor.set([72, 199, 142]);
	});

	it('clamps out-of-range values to [0, 255]', () => {
		loadingColor.set([300, -5, 99]);
		expect(get(loadingColor)).toEqual([255, 0, 99]);
	});

	it('clamps non-numeric values to 0', () => {
		loadingColor.set([NaN, 'abc' as any, undefined as any]);
		expect(get(loadingColor)).toEqual([0, 0, 0]);
	});

	it('rounds fractional inputs', () => {
		loadingColor.set([12.7, 200.4, 50]);
		expect(get(loadingColor)).toEqual([13, 200, 50]);
	});
});

describe('formatIpcError', () => {
	it('returns the message from an Error instance', () => {
		expect(formatIpcError(new Error('boom'))).toBe('boom');
	});

	it('returns a string error unchanged', () => {
		expect(formatIpcError('already a string')).toBe('already a string');
	});

	it('falls back to a generic label for unknown shapes', () => {
		expect(formatIpcError({ weird: true })).toBe('unknown error');
		expect(formatIpcError(undefined)).toBe('unknown error');
		expect(formatIpcError(null)).toBe('unknown error');
	});
});

// Regression test for #600: focailOpen must be reset to false when the
// viewport transitions from mobile to desktop so the Language Hints button
// doesn't stay in a permanently-pressed-but-invisible state.
describe('focailOpen store (regression #600)', () => {
	beforeEach(() => {
		focailOpen.set(false);
	});

	it('starts as false', () => {
		expect(get(focailOpen)).toBe(false);
	});

	it('can be toggled on (simulating mobile button press)', () => {
		focailOpen.set(true);
		expect(get(focailOpen)).toBe(true);
	});

	it('is reset to false on mobile→desktop transition (the #600 fix)', () => {
		// Simulate mobile: user opens the Focail panel
		focailOpen.set(true);
		expect(get(focailOpen)).toBe(true);

		// Simulate desktop: the media query onChange handler fires with matches=false
		// and calls focailOpen.set(false) to avoid a permanently-broken button state.
		focailOpen.set(false);
		expect(get(focailOpen)).toBe(false);
	});
});
