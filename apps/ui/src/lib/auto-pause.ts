/**
 * Frontend auto-pause tracker.
 *
 * Watches for player keyboard / mouse / touch activity and dispatches
 * `/pause` after a configurable idle interval. When the player becomes
 * active again the tracker dispatches `/resume`, but ONLY if the world
 * was paused by this tracker — manual pauses stay sticky.
 *
 * The server-side `tick_inactivity` backstop in parish-server still runs
 * for the tab-close case; the two cooperate without coordination because
 * both check `world.clock.is_paused()` before acting.
 */

export interface AutoPauseTrackerOptions {
	/** Idle interval in milliseconds before auto-pause fires. */
	idleMs: number;
	/** Throttle window for mousemove activity events, in milliseconds. */
	mousemoveThrottleMs: number;
	/** Function to invoke pause/resume commands. */
	submitInput: (text: string) => Promise<void>;
	/** Returns whether the world is currently player-paused. */
	isWorldPaused: () => boolean;
}

export interface AutoPauseTracker {
	/** Record any keyboard / mouse / touch activity. */
	recordActivity: () => void;
	/** Throttled mousemove activity. */
	recordMousemove: () => void;
	/** Notify the tracker that the world's pause state changed. */
	onWorldStateChange: (paused: boolean) => void;
	/** Returns true between an auto-pause and the next resume. */
	wasAutoPaused: () => boolean;
	/** Stop the timer and release internal state. */
	dispose: () => void;
}

export function createAutoPauseTracker(opts: AutoPauseTrackerOptions): AutoPauseTracker {
	let pausedByAutoIdle = false;
	let idleTimer: ReturnType<typeof setTimeout> | null = null;
	let lastMousemoveAt = 0;

	function clearIdleTimer() {
		if (idleTimer !== null) {
			clearTimeout(idleTimer);
			idleTimer = null;
		}
	}

	function startIdleTimer() {
		clearIdleTimer();
		idleTimer = setTimeout(() => {
			idleTimer = null;
			// Only fire pause if the world is currently running.
			if (!opts.isWorldPaused()) {
				pausedByAutoIdle = true;
				void opts.submitInput('/pause');
			}
		}, opts.idleMs);
	}

	function recordActivity() {
		// If we previously auto-paused and the world is still paused, resume.
		if (pausedByAutoIdle && opts.isWorldPaused()) {
			pausedByAutoIdle = false;
			void opts.submitInput('/resume');
		} else if (pausedByAutoIdle && !opts.isWorldPaused()) {
			// World was unpaused some other way (manual /resume); just clear the flag.
			pausedByAutoIdle = false;
		}
		startIdleTimer();
	}

	function recordMousemove() {
		const now = Date.now();
		if (now - lastMousemoveAt < opts.mousemoveThrottleMs) {
			return;
		}
		lastMousemoveAt = now;
		recordActivity();
	}

	function onWorldStateChange(paused: boolean) {
		// If the player manually unpaused, clear our auto-pause flag so
		// any future resume isn't misattributed to us.
		if (!paused) {
			pausedByAutoIdle = false;
		}
	}

	function wasAutoPaused() {
		return pausedByAutoIdle;
	}

	function dispose() {
		clearIdleTimer();
	}

	// Start the timer immediately so the first idle interval counts down.
	startIdleTimer();

	return {
		recordActivity,
		recordMousemove,
		onWorldStateChange,
		wasAutoPaused,
		dispose
	};
}
