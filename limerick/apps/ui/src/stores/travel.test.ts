import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { travelState, startTravel, cancelTravel } from './travel';
import type { TravelStartPayload, TravelWaypoint } from '$lib/types';

const waypoints: TravelWaypoint[] = [
	{ id: 'loc1', lat: 53.35, lon: -6.26 },
	{ id: 'loc2', lat: 53.39, lon: -6.07 },
	{ id: 'loc3', lat: 53.45, lon: -5.9 },
];

const payload: TravelStartPayload = {
	waypoints,
	duration_minutes: 10,
	destination: 'loc3',
};

describe('travelState', () => {
	beforeEach(() => {
		travelState.set(null);
		vi.useFakeTimers();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it('starts null', () => {
		expect(get(travelState)).toBeNull();
	});

	it('startTravel sets travel state with computed animationMs', () => {
		startTravel(payload);
		const state = get(travelState);
		expect(state).not.toBeNull();
		expect(state!.waypoints).toEqual(waypoints);
		expect(state!.destination).toBe('loc3');
		expect(state!.durationMinutes).toBe(10);
		expect(typeof state!.startedAt).toBe('number');
		// 10 min * 150 ms/min = 1500ms
		expect(state!.animationMs).toBe(1500);
	});

	it('startTravel clamps animationMs to minimum', () => {
		startTravel({ ...payload, duration_minutes: 1 });
		expect(get(travelState)!.animationMs).toBe(600);
	});

	it('startTravel clamps animationMs to maximum', () => {
		startTravel({ ...payload, duration_minutes: 100 });
		expect(get(travelState)!.animationMs).toBe(3000);
	});

	it('startTravel ignores payload with fewer than 2 waypoints', () => {
		startTravel({ ...payload, waypoints: [{ id: 'loc1', lat: 0, lon: 0 }] });
		expect(get(travelState)).toBeNull();
	});

	it('auto-clears travel state after animationMs elapses', () => {
		startTravel(payload);
		expect(get(travelState)).not.toBeNull();

		vi.advanceTimersByTime(1500);
		expect(get(travelState)).toBeNull();
	});

	it('cancelTravel clears state and cancels auto-clear timer', () => {
		startTravel(payload);
		expect(get(travelState)).not.toBeNull();

		cancelTravel();
		expect(get(travelState)).toBeNull();

		// Advance past the auto-clear time -- should not set anything
		vi.advanceTimersByTime(1500);
		expect(get(travelState)).toBeNull();
	});

	it('second startTravel cancels prior auto-clear timer (#349)', () => {
		// First travel with short duration so its timer would fire early
		startTravel({ ...payload, duration_minutes: 1 }); // clamped to 600ms
		expect(get(travelState)).not.toBeNull();

		// Second travel starts before first expires — must cancel first timer
		const payload2 = { ...payload, duration_minutes: 10 };
		startTravel(payload2);
		expect(get(travelState)!.animationMs).toBe(1500);

		// Advance to where the FIRST timer would have fired (600ms).
		// State should still be set because clearPendingTravelReset cancelled it.
		vi.advanceTimersByTime(600);
		expect(get(travelState)).not.toBeNull();

		// Advance to where the SECOND timer fires.
		vi.advanceTimersByTime(900);
		expect(get(travelState)).toBeNull();
	});
});
