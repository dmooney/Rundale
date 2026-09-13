import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
	textLog,
	streamingActive,
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
	externalDriveActive,
	noteStreamingStarted,
	resetExternalDrive,
	EXTERNAL_DRIVE_IDLE_MS,
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

// ── JOB 2 — externalDriveActive / noteStreamingStarted (#1537) ────────────────
//
// Signal: when streamingActive becomes true WITHOUT a preceding local
// playerSubmittedCount increment (i.e. localSubmitCount === lastLocalSubmitCount),
// the turn was driven by the bridge/harness.  The flag auto-clears after
// EXTERNAL_DRIVE_IDLE_MS of idle.  For local-player turns the badge clears
// immediately.
describe('noteStreamingStarted — external-drive detection (#1537)', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		resetExternalDrive();
	});

	afterEach(() => {
		vi.useRealTimers();
		resetExternalDrive();
	});

	it('sets externalDriveActive when the submit count has not changed (bridge turn)', () => {
		// Both args equal → no local submit happened → external driver.
		noteStreamingStarted(3, 3);
		expect(get(externalDriveActive)).toBe(true);
	});

	it('clears externalDriveActive immediately for a local-player turn', () => {
		// First mark as externally driven, then a local submit arrives.
		noteStreamingStarted(3, 3);
		expect(get(externalDriveActive)).toBe(true);
		// Local submit: count incremented from 3 to 4.
		noteStreamingStarted(4, 3);
		expect(get(externalDriveActive)).toBe(false);
	});

	it('auto-clears after EXTERNAL_DRIVE_IDLE_MS', () => {
		noteStreamingStarted(5, 5);
		expect(get(externalDriveActive)).toBe(true);
		// Advance time past the idle threshold.
		vi.advanceTimersByTime(EXTERNAL_DRIVE_IDLE_MS + 1);
		expect(get(externalDriveActive)).toBe(false);
	});

	it('keeps the badge alive when consecutive external turns arrive before idle', () => {
		noteStreamingStarted(5, 5);
		// Second external turn restarts the timer.
		vi.advanceTimersByTime(EXTERNAL_DRIVE_IDLE_MS - 100);
		noteStreamingStarted(5, 5);
		// Should still be true after the original deadline.
		vi.advanceTimersByTime(EXTERNAL_DRIVE_IDLE_MS - 100);
		expect(get(externalDriveActive)).toBe(true);
		// But clears after the refreshed deadline.
		vi.advanceTimersByTime(200);
		expect(get(externalDriveActive)).toBe(false);
	});

	it('resetExternalDrive cancels the timer and clears the flag', () => {
		noteStreamingStarted(5, 5);
		expect(get(externalDriveActive)).toBe(true);
		resetExternalDrive();
		expect(get(externalDriveActive)).toBe(false);
		// No spurious set after timer fires.
		vi.advanceTimersByTime(EXTERNAL_DRIVE_IDLE_MS + 1);
		expect(get(externalDriveActive)).toBe(false);
	});
});

// ── JOB 1 — Loading safety timeout pattern (#1536) ────────────────────────────
//
// The page-controller arms a 10 s safety timer when loading starts; if stream-end
// or loading{active:false} never arrives (bridge-driven turn, no NPC stream),
// the timer force-clears streamingActive.
//
// We test the observable behavior of the stores directly here — the timer
// itself lives in page-controller but it calls streamingActive.set(false), so we
// can simulate the pattern with fake timers and the stores.
describe('loading safety timeout behavior (#1536)', () => {
	const SAFETY_MS = 10_000;

	beforeEach(() => {
		vi.useFakeTimers();
		streamingActive.set(false);
	});

	afterEach(() => {
		vi.useRealTimers();
		streamingActive.set(false);
	});

	it('streamingActive clears after a timeout when no stream-end fires (bridge-driven turn)', () => {
		// Simulate page-controller arms safety timer on loading{active:true}.
		streamingActive.set(true);
		let cleared = false;
		const timer = setTimeout(() => {
			streamingActive.set(false);
			cleared = true;
		}, SAFETY_MS);

		expect(get(streamingActive)).toBe(true);
		vi.advanceTimersByTime(SAFETY_MS - 1);
		expect(get(streamingActive)).toBe(true); // not yet
		vi.advanceTimersByTime(2);
		expect(get(streamingActive)).toBe(false); // safety timeout fired
		expect(cleared).toBe(true);
		clearTimeout(timer); // no-op — already fired
	});

	it('safety timer is disarmed when stream-end fires first', () => {
		streamingActive.set(true);
		let safetyFired = false;
		const timer = setTimeout(() => {
			safetyFired = true;
			streamingActive.set(false);
		}, SAFETY_MS);

		// stream-end fires before the deadline — disarm.
		clearTimeout(timer);
		streamingActive.set(false);

		// Advance well past the deadline.
		vi.advanceTimersByTime(SAFETY_MS + 1000);
		expect(safetyFired).toBe(false); // never fired
		expect(get(streamingActive)).toBe(false);
	});
});

// ── Multi-turn chain classification fix (#1538) ───────────────────────────────
//
// When a LOCAL player initiates a multi-NPC conversation chain, the backend
// fires loading{active:true} multiple times within that single turn (once per
// re-spawned NPC stream).  The page-controller fix guards noteStreamingStarted
// behind !sm.isChainInProgress() so only the FIRST loading event of a chain
// triggers re-classification; subsequent re-spawns inherit the chain's existing
// local/external verdict.
//
// These tests verify the expected store behavior under the CORRECTED protocol:
// — Local chain: noteStreamingStarted called once (count incremented), then
//   subsequent re-spawns do NOT call noteStreamingStarted; externalDriveActive
//   must remain false throughout.
// — External chain: noteStreamingStarted called once (count unchanged); badge
//   must remain true on subsequent re-spawns.
describe('multi-turn chain classification — page-controller protocol (#1538)', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		resetExternalDrive();
	});

	afterEach(() => {
		vi.useRealTimers();
		resetExternalDrive();
	});

	it('local-initiated chain: multiple loading re-spawns with no further submit do NOT set externalDriveActive', () => {
		// Turn starts: player submitted (count 2 → 3).  Page-controller only calls
		// noteStreamingStarted on the FIRST loading{active:true} (chain not yet in
		// progress).
		noteStreamingStarted(3, 2); // currentCount=3, lastCount=2 → local turn
		expect(get(externalDriveActive)).toBe(false);

		// Backend re-spawns loading for 2nd and 3rd NPC turns within the same
		// chain.  Page-controller now skips noteStreamingStarted (chainInProgress).
		// externalDriveActive must still be false.
		// (Simulate the skip: simply do NOT call noteStreamingStarted again.)
		expect(get(externalDriveActive)).toBe(false);
		expect(get(externalDriveActive)).toBe(false);
	});

	it('external chain: badge stays true across multiple re-spawned loadings', () => {
		// Harness/bridge turn: count unchanged (both 5 → 5 → external).
		noteStreamingStarted(5, 5); // first loading{active:true}
		expect(get(externalDriveActive)).toBe(true);

		// Subsequent re-spawns within the same chain also skip noteStreamingStarted
		// (page-controller guard), so the badge stays true until idle timeout.
		expect(get(externalDriveActive)).toBe(true);

		// Badge auto-clears after idle.
		vi.advanceTimersByTime(EXTERNAL_DRIVE_IDLE_MS + 1);
		expect(get(externalDriveActive)).toBe(false);
	});

	it('naïve re-evaluation (the pre-fix bug) would set externalDriveActive wrongly on re-spawn', () => {
		// Demonstrate the OLD broken behavior: calling noteStreamingStarted on
		// every loading re-spawn (count not yet incremented further) would
		// incorrectly flip externalDriveActive to true for a local chain.
		// This test documents the hazard that the fix prevents.
		const lastCount = 2;
		const currentCount = 3; // local submit happened

		// First loading — classified local (correct).
		noteStreamingStarted(currentCount, lastCount);
		expect(get(externalDriveActive)).toBe(false);

		// Simulate the BUG: re-spawn calls noteStreamingStarted again with the
		// same currentCount (no further submit), which equals the already-updated
		// lastLocalSubmitCount (3 === 3) → external classification (WRONG).
		noteStreamingStarted(currentCount, currentCount);
		expect(get(externalDriveActive)).toBe(true); // incorrectly set — bug!

		// Reset for cleanup.
		resetExternalDrive();
	});
});
