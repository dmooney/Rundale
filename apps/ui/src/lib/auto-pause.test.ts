import { describe, it, expect, beforeEach, afterEach, vi, type Mock } from 'vitest';
import { createAutoPauseTracker } from './auto-pause';

describe('createAutoPauseTracker', () => {
	let submitInput: Mock<(text: string) => Promise<void>>;
	let worldPaused: boolean;

	beforeEach(() => {
		vi.useFakeTimers();
		submitInput = vi.fn(async (_text: string) => {});
		worldPaused = false;
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	function makeTracker() {
		return createAutoPauseTracker({
			idleMs: 60_000,
			mousemoveThrottleMs: 1000,
			submitInput,
			isWorldPaused: () => worldPaused
		});
	}

	it('auto-pauses after the idle interval with no activity', () => {
		const tracker = makeTracker();
		vi.advanceTimersByTime(60_000);
		expect(submitInput).toHaveBeenCalledWith('/pause');
		expect(tracker.wasAutoPaused()).toBe(true);
		tracker.dispose();
	});

	it('does not auto-pause when activity occurs before the timeout', () => {
		const tracker = makeTracker();
		vi.advanceTimersByTime(30_000);
		tracker.recordActivity();
		vi.advanceTimersByTime(30_000);
		expect(submitInput).not.toHaveBeenCalled();
		tracker.dispose();
	});

	it('auto-resumes when activity occurs after auto-pause', () => {
		const tracker = makeTracker();
		vi.advanceTimersByTime(60_000);
		expect(submitInput).toHaveBeenLastCalledWith('/pause');

		// Simulate the world being paused now.
		worldPaused = true;

		tracker.recordActivity();
		expect(submitInput).toHaveBeenLastCalledWith('/resume');
		expect(tracker.wasAutoPaused()).toBe(false);
		tracker.dispose();
	});

	it('manual pauses are sticky — no auto-resume on activity', () => {
		const tracker = makeTracker();
		// World is paused by some other means (player typed /pause).
		worldPaused = true;
		tracker.onWorldStateChange(true);

		// Activity should NOT trigger a resume because we didn't auto-pause.
		tracker.recordActivity();
		expect(submitInput).not.toHaveBeenCalledWith('/resume');
		tracker.dispose();
	});

	it('mousemove is throttled within the throttle window', () => {
		const tracker = makeTracker();
		vi.advanceTimersByTime(30_000);
		tracker.recordMousemove();
		vi.advanceTimersByTime(500);
		tracker.recordMousemove(); // Within throttle window — ignored.
		// First mousemove reset the timer at t=30_000, so 60s later (t=90_000) it fires.
		vi.advanceTimersByTime(60_000);
		expect(submitInput).toHaveBeenCalledWith('/pause');
		tracker.dispose();
	});

	it('mousemove after the throttle window resets the timer', () => {
		const tracker = makeTracker();
		vi.advanceTimersByTime(30_000);
		tracker.recordMousemove();
		vi.advanceTimersByTime(2000); // Past the throttle window.
		tracker.recordMousemove(); // Should reset the timer.
		vi.advanceTimersByTime(50_000);
		expect(submitInput).not.toHaveBeenCalled();
		vi.advanceTimersByTime(15_000);
		expect(submitInput).toHaveBeenCalledWith('/pause');
		tracker.dispose();
	});

	it('dispose clears the pending timer', () => {
		const tracker = makeTracker();
		tracker.dispose();
		vi.advanceTimersByTime(120_000);
		expect(submitInput).not.toHaveBeenCalled();
	});

	it('onWorldStateChange clears the auto-pause flag if user manually resumes', () => {
		const tracker = makeTracker();
		vi.advanceTimersByTime(60_000);
		expect(tracker.wasAutoPaused()).toBe(true);

		// User manually resumes — onWorldStateChange should clear the flag.
		worldPaused = false;
		tracker.onWorldStateChange(false);
		expect(tracker.wasAutoPaused()).toBe(false);
		tracker.dispose();
	});

	it('wasAutoPaused returns true between auto-pause and resume', () => {
		const tracker = makeTracker();
		expect(tracker.wasAutoPaused()).toBe(false);

		vi.advanceTimersByTime(60_000);
		expect(tracker.wasAutoPaused()).toBe(true);

		worldPaused = true;
		tracker.recordActivity();
		expect(tracker.wasAutoPaused()).toBe(false);
		tracker.dispose();
	});
});
