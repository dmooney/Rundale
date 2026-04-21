import { describe, it, expect } from 'vitest';
import { positionAlongPath } from './controller';
import type { TravelWaypoint } from '$lib/types';

function buildSegments(waypoints: TravelWaypoint[]): { segs: number[]; total: number } {
	const segs: number[] = [];
	let total = 0;
	for (let i = 1; i < waypoints.length; i += 1) {
		const a = waypoints[i - 1];
		const b = waypoints[i];
		const d = Math.hypot(b.lon - a.lon, b.lat - a.lat);
		segs.push(d);
		total += d;
	}
	return { segs, total };
}

describe('positionAlongPath', () => {
	const straight: TravelWaypoint[] = [
		{ id: 'a', lat: 0, lon: 0 },
		{ id: 'b', lat: 0, lon: 10 }
	];

	it('returns first waypoint at t=0', () => {
		const { segs, total } = buildSegments(straight);
		expect(positionAlongPath(straight, segs, total, 0)).toEqual([0, 0]);
	});

	it('returns last waypoint at t=1', () => {
		const { segs, total } = buildSegments(straight);
		expect(positionAlongPath(straight, segs, total, 1)).toEqual([10, 0]);
	});

	it('returns midpoint at t=0.5 for a single segment', () => {
		const { segs, total } = buildSegments(straight);
		const [lon, lat] = positionAlongPath(straight, segs, total, 0.5);
		expect(lon).toBeCloseTo(5, 10);
		expect(lat).toBeCloseTo(0, 10);
	});

	it('clamps t > 1 to the last waypoint', () => {
		const { segs, total } = buildSegments(straight);
		expect(positionAlongPath(straight, segs, total, 1.5)).toEqual([10, 0]);
	});

	it('weights multi-segment progress by distance', () => {
		// Two legs of different lengths: short horizontal, long vertical.
		const path: TravelWaypoint[] = [
			{ id: 'a', lat: 0, lon: 0 },
			{ id: 'b', lat: 0, lon: 1 },
			{ id: 'c', lat: 9, lon: 1 }
		];
		const { segs, total } = buildSegments(path);
		expect(total).toBeCloseTo(10, 10);

		// At t=0.1, we've travelled 1 unit — exactly the first leg.
		const [lonMid, latMid] = positionAlongPath(path, segs, total, 0.1);
		expect(lonMid).toBeCloseTo(1, 10);
		expect(latMid).toBeCloseTo(0, 10);

		// At t=0.5, we've travelled 5 units — 1 on leg 1 + 4 up leg 2.
		const [lonHalf, latHalf] = positionAlongPath(path, segs, total, 0.5);
		expect(lonHalf).toBeCloseTo(1, 10);
		expect(latHalf).toBeCloseTo(4, 10);
	});

	it('handles a degenerate zero-length path', () => {
		const degenerate: TravelWaypoint[] = [
			{ id: 'a', lat: 3, lon: 7 },
			{ id: 'b', lat: 3, lon: 7 }
		];
		const { segs, total } = buildSegments(degenerate);
		expect(positionAlongPath(degenerate, segs, total, 0.4)).toEqual([7, 3]);
	});
});
