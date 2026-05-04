import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { textLog, pushErrorLog, formatIpcError, loadingColor, focailOpen, syncFocailOnViewportChange } from './game';

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
//
// These tests exercise syncFocailOnViewportChange — the function called by
// the matchMedia onChange handler in +page.svelte — rather than the writable
// store directly. A test that calls focailOpen.set(false) manually would pass
// even if the handler were deleted; these tests fail if the handler logic is
// removed or inverted.
describe('syncFocailOnViewportChange (regression #600)', () => {
	beforeEach(() => {
		focailOpen.set(false);
	});

	it('resets focailOpen to false when transitioning to desktop (matches=false)', () => {
		// Simulate: user opened the Focail panel on mobile.
		focailOpen.set(true);
		expect(get(focailOpen)).toBe(true);

		// Simulate: matchMedia onChange fires with e.matches=false (now desktop).
		// syncFocailOnViewportChange must reset the store so the button is not
		// left in a permanently-pressed-but-invisible state.
		syncFocailOnViewportChange(false);
		expect(get(focailOpen)).toBe(false);
	});

	it('does NOT reset focailOpen when transitioning to mobile (matches=true)', () => {
		// Simulate: user opened the panel on mobile, viewport shrinks further.
		focailOpen.set(true);
		expect(get(focailOpen)).toBe(true);

		// matches=true means the narrow-viewport query still matches; the mobile
		// branch is still active so focailOpen should be left unchanged.
		syncFocailOnViewportChange(true);
		expect(get(focailOpen)).toBe(true);
	});

	it('is a no-op when focailOpen is already false and viewport goes desktop', () => {
		// Store is already false; going to desktop should leave it false.
		expect(get(focailOpen)).toBe(false);
		syncFocailOnViewportChange(false);
		expect(get(focailOpen)).toBe(false);
	});
});
