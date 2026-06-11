import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
	textLog,
	pushErrorLog,
	formatIpcError,
	loadingColor,
	focailOpen,
	syncFocailOnViewportChange,
	messageHints,
	pruneMessageHints,
	trimTextLog,
	noteTimeRule,
	resetTimeRule,
} from './game';
import type { LanguageHint, WorldSnapshot } from '$lib/types';

const hint: LanguageHint[] = [
	{ word: 'dia dhuit', pronunciation: 'jee-ah gwit', meaning: 'hello' },
];

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
			content: 'Something went wrong',
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

describe('messageHints eviction (audit H3)', () => {
	beforeEach(() => {
		messageHints.set(new Map());
		textLog.set([]);
	});

	it('drops hint entries whose message is no longer in the log', () => {
		messageHints.set(
			new Map([
				['m1', hint],
				['m2', hint],
			]),
		);
		// Only m2 survives in the log.
		pruneMessageHints([{ id: 'm2', source: 'Saoirse', content: 'hi' }]);
		const m = get(messageHints);
		expect(m.has('m1')).toBe(false);
		expect(m.has('m2')).toBe(true);
	});

	it('keeps all entries when every keyed message is still present', () => {
		messageHints.set(new Map([['m1', hint]]));
		pruneMessageHints([{ id: 'm1', source: 'Saoirse', content: 'hi' }]);
		expect(get(messageHints).size).toBe(1);
	});

	it('stays bounded once textLog is trimmed past its cap (no unbounded growth)', () => {
		// Simulate a long session: 600 NPC turns, each appended to the log AND
		// recorded in messageHints. The textLog subscriber prunes on every
		// trimmed update, so messageHints must not exceed the surviving log.
		for (let i = 0; i < 600; i++) {
			const id = `turn-${i}`;
			textLog.update((log) =>
				trimTextLog([...log, { id, source: 'Saoirse', content: 'x' }]),
			);
			messageHints.update((m) => {
				m.set(id, hint);
				return m;
			});
		}
		const hints = get(messageHints);
		const log = get(textLog);
		// Without eviction this would be 600; bounded to the live (trimmed) log.
		expect(hints.size).toBeLessThanOrEqual(log.length);
		expect(hints.size).toBeLessThan(600);
		// Every retained hint key must correspond to a live log entry.
		const liveIds = new Set(log.map((e) => e.id));
		for (const key of hints.keys()) {
			expect(liveIds.has(key)).toBe(true);
		}
	});
});

describe('trimTextLog (TD-049)', () => {
	const makeLog = (n: number) =>
		Array.from({ length: n }, (_, i) => ({
			id: `e${i}`,
			source: 'You',
			content: String(i),
		}));

	it('is a no-op when the log is below the cap', () => {
		const log = makeLog(100);
		const out = trimTextLog(log);
		expect(out).toBe(log); // same reference — not copied
		expect(out.length).toBe(100);
	});

	it('is a no-op at exactly the cap (500 stays 500)', () => {
		const log = makeLog(500);
		const out = trimTextLog(log);
		expect(out).toBe(log);
		expect(out.length).toBe(500);
	});

	it('trims 501 down to 500, dropping the single oldest entry', () => {
		const log = makeLog(501);
		const out = trimTextLog(log);
		expect(out.length).toBe(500);
		// Oldest (e0) dropped; newest retained.
		expect(out[0].id).toBe('e1');
		expect(out[out.length - 1].id).toBe('e500');
	});

	it('trims 1000 down to 500, keeping the newest 500', () => {
		const log = makeLog(1000);
		const out = trimTextLog(log);
		expect(out.length).toBe(500);
		expect(out[0].id).toBe('e500');
		expect(out[out.length - 1].id).toBe('e999');
	});
});

describe('noteTimeRule (time-of-day separators)', () => {
	function snap(time_label: string, day_of_week: string): WorldSnapshot {
		return { time_label, day_of_week } as unknown as WorldSnapshot;
	}

	beforeEach(() => {
		resetTimeRule();
		textLog.set([{ source: 'system', content: 'You arrive.' }]);
	});

	it('primes on the first snapshot without emitting a rule', () => {
		noteTimeRule(snap('Morning', 'Monday'));
		expect(get(textLog).length).toBe(1);
	});

	it('emits nothing while the period is unchanged', () => {
		noteTimeRule(snap('Morning', 'Monday'));
		noteTimeRule(snap('Morning', 'Monday'));
		noteTimeRule(snap('Morning', 'Monday'));
		expect(get(textLog).length).toBe(1);
	});

	it('appends a time-rule entry when the period changes', () => {
		noteTimeRule(snap('Morning', 'Monday'));
		noteTimeRule(snap('Midday', 'Monday'));
		const log = get(textLog);
		expect(log.length).toBe(2);
		expect(log[1].subtype).toBe('time-rule');
		expect(log[1].content).toBe('Midday — Monday');
	});

	it('appends a rule when the day changes even within the same period label', () => {
		noteTimeRule(snap('Night', 'Monday'));
		noteTimeRule(snap('Night', 'Tuesday'));
		const log = get(textLog);
		expect(log.length).toBe(2);
		expect(log[1].content).toBe('Night — Tuesday');
	});

	it('never inserts a rule into an empty log (nothing precedes the splash)', () => {
		textLog.set([]);
		noteTimeRule(snap('Morning', 'Monday'));
		noteTimeRule(snap('Midday', 'Monday'));
		expect(get(textLog).length).toBe(0);
	});

	it('ignores snapshots without a time label', () => {
		noteTimeRule(snap('', 'Monday'));
		noteTimeRule(snap('Morning', 'Monday'));
		expect(get(textLog).length).toBe(1);
	});

	it('a null snapshot resets tracking so the next snapshot primes silently', () => {
		noteTimeRule(snap('Morning', 'Monday'));
		// World state cleared (new game / branch switch / teardown).
		noteTimeRule(null);
		// Without the reset this would emit a Midday rule from the stale key.
		noteTimeRule(snap('Midday', 'Tuesday'));
		expect(get(textLog).length).toBe(1);
		// Subsequent change after re-priming emits normally.
		noteTimeRule(snap('Dusk', 'Tuesday'));
		const log = get(textLog);
		expect(log.length).toBe(2);
		expect(log[1].content).toBe('Dusk — Tuesday');
	});
});
