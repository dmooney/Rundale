import { writable, derived } from 'svelte/store';
import type { TravelStartPayload, TravelWaypoint } from '$lib/types';

/** Current travel animation state (null when not traveling). */
export interface TravelState {
	/** Ordered waypoints from origin to destination. */
	waypoints: TravelWaypoint[];
	/** Total travel duration in game minutes. */
	durationMinutes: number;
	/** Destination location ID. */
	destination: string;
	/** Wall-clock timestamp (ms) when animation started. */
	startedAt: number;
	/** Animation duration in real milliseconds. */
	animationMs: number;
}

/** How long the travel animation plays in real milliseconds, per game minute. */
const MS_PER_GAME_MINUTE = 150;
/** Minimum animation duration. */
const MIN_ANIMATION_MS = 600;
/** Maximum animation duration. */
const MAX_ANIMATION_MS = 3000;

/** The active travel animation, or null if idle. */
export const travelState = writable<TravelState | null>(null);

/** Whether a travel animation is currently playing. */
export const isTraveling = derived(travelState, ($t) => $t !== null);

/** Outstanding auto-clear timer for the active travel animation.
 *
 * #349: each call to startTravel must cancel the prior reset timer.
 * Without this, two travels that overlap (server emitting nested
 * location-entry events before the first animation finishes) leave
 * the earlier setTimeout running. When that earlier timer fires it
 * clears travelState mid-animation of the *newer* travel, freezing
 * the path-following dot in MapController.
 *
 * Exposed at module scope so the cleanup runs even when startTravel
 * is invoked from outside any component lifecycle.
 */
let travelResetTimer: ReturnType<typeof setTimeout> | null = null;

/** Cancels any outstanding auto-clear timer for the travel animation.
 *
 * Shared by `startTravel` (so a new travel can't be torn down by the
 * prior timer) and `cancelTravel` (unmount path). Extracted per a
 * code-review nit on #583 to avoid duplicating the null-and-clear
 * dance at every call site.
 */
function clearPendingTravelReset(): void {
	if (travelResetTimer !== null) {
		clearTimeout(travelResetTimer);
		travelResetTimer = null;
	}
}

/** Starts a travel animation from a TravelStartPayload. */
export function startTravel(payload: TravelStartPayload): void {
	if (payload.waypoints.length < 2) return;

	// Cancel any pending auto-clear from a prior travel so it can't
	// fire after we've installed the new state (#349).
	clearPendingTravelReset();

	const raw = payload.duration_minutes * MS_PER_GAME_MINUTE;
	const animationMs = Math.max(MIN_ANIMATION_MS, Math.min(MAX_ANIMATION_MS, raw));

	travelState.set({
		waypoints: payload.waypoints,
		durationMinutes: payload.duration_minutes,
		destination: payload.destination,
		startedAt: performance.now(),
		animationMs
	});

	// Auto-clear when animation completes
	travelResetTimer = setTimeout(() => {
		travelState.set(null);
		travelResetTimer = null;
	}, animationMs);
}

/** Cancels any pending auto-clear timer and clears travel state.
 *
 * Called by the host component on unmount (#349) so a stale timer
 * doesn't keep the travel store referencing payload data after the
 * UI is gone, and doesn't fire travelState.set(null) into a destroyed
 * tree.
 */
export function cancelTravel(): void {
	clearPendingTravelReset();
	travelState.set(null);
}

/**
 * Computes the current interpolated position along the travel path.
 *
 * Returns `{ lat, lon, progress, segmentIndex }` where progress is 0–1
 * and segmentIndex is which edge segment the dot is currently on.
 * Returns null if not traveling.
 */
export function getTravelPosition(
	state: TravelState,
	now: number
): { lat: number; lon: number; progress: number; segmentIndex: number } | null {
	const elapsed = now - state.startedAt;
	const progress = Math.min(1, Math.max(0, elapsed / state.animationMs));

	const wps = state.waypoints;
	const segCount = wps.length - 1;
	if (segCount < 1) return null;

	// Map progress to segment
	const segFloat = progress * segCount;
	const segIndex = Math.min(Math.floor(segFloat), segCount - 1);
	const segProgress = segFloat - segIndex;

	const from = wps[segIndex];
	const to = wps[segIndex + 1];

	return {
		lat: from.lat + (to.lat - from.lat) * segProgress,
		lon: from.lon + (to.lon - from.lon) * segProgress,
		progress,
		segmentIndex: segIndex
	};
}
