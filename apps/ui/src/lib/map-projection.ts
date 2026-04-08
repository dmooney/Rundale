/**
 * Shared map projection utilities for the minimap and full map overlay.
 *
 * Uses a fixed-scale mercator-like projection centered on the parish area.
 * Both MapPanel (minimap) and FullMapOverlay share the same coordinate system
 * so positions are consistent across views.
 */

import type { MapLocation } from '$lib/types';

/** Approximate center latitude of the parish (Kiltoom/Kilteevan area). */
export const REF_CENTER_LAT = 53.59;

/** Approximate center longitude of the parish. */
export const REF_CENTER_LON = -8.15;

/** Pixels per degree — controls the world-space scale. */
export const SCALE = 8000;

/** A location with projected world-space coordinates. */
export interface ProjectedLocation extends MapLocation {
	x: number;
	y: number;
}

/**
 * Projects locations from WGS-84 coordinates into world-space pixel coordinates.
 *
 * Uses a fixed reference point and scale so the coordinate system is stable
 * regardless of which subset of locations is being displayed. Falls back to
 * a grid layout when no locations have coordinates.
 */
export function projectWorld(locs: MapLocation[]): ProjectedLocation[] {
	const hasCoords = locs.some((l) => l.lat !== 0 || l.lon !== 0);
	if (!hasCoords || locs.length === 0) {
		// Grid fallback layout — 5 columns, 200px spacing
		return locs.map((l, i) => ({
			...l,
			x: ((i % 5) + 0.5) * 200,
			y: (Math.floor(i / 5) + 0.5) * 200
		}));
	}

	const cosLat = Math.cos(REF_CENTER_LAT * (Math.PI / 180));
	return locs.map((l) => ({
		...l,
		x: (l.lon - REF_CENTER_LON) * SCALE * cosLat,
		y: (REF_CENTER_LAT - l.lat) * SCALE
	}));
}

/** Result of clamping a point to a rectangle boundary. */
export interface ClampResult {
	x: number;
	y: number;
	/** Angle in radians from the rectangle center toward the original point. */
	angle: number;
}

/**
 * Clamps a point to the boundary of a rectangle, returning the clamped
 * position and the angle from the center of the rectangle toward the point.
 *
 * Used for off-screen indicators on the minimap.
 */
export function clampToRect(
	px: number,
	py: number,
	cx: number,
	cy: number,
	halfW: number,
	halfH: number
): ClampResult {
	const angle = Math.atan2(py - cy, px - cx);

	// Ray-rectangle intersection
	const cosA = Math.cos(angle);
	const sinA = Math.sin(angle);

	let t = Infinity;
	if (cosA !== 0) {
		const tx = (cosA > 0 ? halfW : -halfW) / cosA;
		if (tx > 0) t = Math.min(t, tx);
	}
	if (sinA !== 0) {
		const ty = (sinA > 0 ? halfH : -halfH) / sinA;
		if (ty > 0) t = Math.min(t, ty);
	}

	return {
		x: cx + cosA * t,
		y: cy + sinA * t,
		angle
	};
}
