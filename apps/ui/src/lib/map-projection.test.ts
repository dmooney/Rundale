import { describe, it, expect } from 'vitest';
import { projectWorld, clampToRect, REF_CENTER_LAT, REF_CENTER_LON } from './map-projection';
import type { MapLocation } from '$lib/types';

function makeLoc(overrides: Partial<MapLocation> = {}): MapLocation {
	return {
		id: 'test',
		name: 'Test',
		lat: REF_CENTER_LAT,
		lon: REF_CENTER_LON,
		adjacent: false,
		hops: 0,
		...overrides
	};
}

describe('projectWorld', () => {
	it('returns grid layout when no locations have coordinates', () => {
		const locs = [
			makeLoc({ id: 'a', lat: 0, lon: 0 }),
			makeLoc({ id: 'b', lat: 0, lon: 0 })
		];
		const result = projectWorld(locs);
		expect(result.length).toBe(2);
		// Grid fallback: first item at column 0, row 0
		expect(result[0].x).toBeCloseTo(100);
		expect(result[0].y).toBeCloseTo(100);
	});

	it('projects locations to world-space coordinates', () => {
		const locs = [
			makeLoc({ id: 'center', lat: REF_CENTER_LAT, lon: REF_CENTER_LON }),
			makeLoc({ id: 'east', lat: REF_CENTER_LAT, lon: REF_CENTER_LON + 0.01 })
		];
		const result = projectWorld(locs);
		// Center should be at (0, 0)
		expect(result[0].x).toBeCloseTo(0);
		expect(result[0].y).toBeCloseTo(0);
		// East should have positive x
		expect(result[1].x).toBeGreaterThan(0);
		expect(result[1].y).toBeCloseTo(0);
	});

	it('projects latitude inversely (north = lower y)', () => {
		const locs = [
			makeLoc({ id: 'south', lat: REF_CENTER_LAT - 0.01 }),
			makeLoc({ id: 'north', lat: REF_CENTER_LAT + 0.01 })
		];
		const result = projectWorld(locs);
		// North has lower y (negative), south has higher y (positive)
		expect(result[1].y).toBeLessThan(result[0].y);
	});

	it('returns empty array for empty input', () => {
		expect(projectWorld([])).toEqual([]);
	});
});

describe('clampToRect', () => {
	it('clamps a point to the right edge', () => {
		const result = clampToRect(200, 0, 0, 0, 100, 100);
		expect(result.x).toBeCloseTo(100);
		expect(result.y).toBeCloseTo(0);
	});

	it('clamps a point to the bottom edge', () => {
		const result = clampToRect(0, 200, 0, 0, 100, 100);
		expect(result.x).toBeCloseTo(0);
		expect(result.y).toBeCloseTo(100);
	});

	it('returns angle pointing toward the original point', () => {
		const result = clampToRect(200, 0, 0, 0, 100, 100);
		// Angle should be ~0 (pointing right)
		expect(result.angle).toBeCloseTo(0, 1);
	});

	it('clamps diagonal points to corner region', () => {
		const result = clampToRect(200, 200, 0, 0, 100, 100);
		// Should be clamped to one of the edges
		expect(result.x).toBeLessThanOrEqual(100);
		expect(result.y).toBeLessThanOrEqual(100);
	});
});
