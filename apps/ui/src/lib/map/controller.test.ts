import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { positionAlongPath, drawIconImage } from './controller';
import type { TravelWaypoint } from '$lib/types';

// ─── drawIconImage ────────────────────────────────────────────────────────────
//
// Regression test for issue #407: a Y-axis flip (translate(0,size) +
// scale(…,−…)) in drawIconImage would render Phosphor icons upside-down.
// These tests verify the canvas transform calls are correct (positive Y scale,
// no vertical translate) and that filled pixels land in the top rows of the
// canvas when the source path data is top-anchored.

// A minimal square path in the top-left quadrant of a 256×256 Phosphor viewBox.
const TOP_LEFT_SQUARE = 'M0,0 H128 V128 H0 Z';

describe('drawIconImage', () => {
	const SIZE = 64;

	// Track transform calls made on the fake 2D context.
	let scaleArgs: Array<[number, number]> = [];
	let translateArgs: Array<[number, number]> = [];

	// Pixel buffer reused across tests.
	let pixels: Uint8ClampedArray;

	beforeEach(() => {
		scaleArgs = [];
		translateArgs = [];
		pixels = new Uint8ClampedArray(SIZE * SIZE * 4);

		// Provide Path2D in the jsdom global if it is missing.
		if (typeof globalThis.Path2D === 'undefined') {
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			(globalThis as any).Path2D = class {
				constructor(_d?: string) {}
			};
		}

		// Inject a fake canvas that records transform calls.
		const origCreate = document.createElement.bind(document);
		vi.spyOn(document, 'createElement').mockImplementation((tag: string) => {
			if (tag !== 'canvas') return origCreate(tag);

			const ctx = {
				clearRect: vi.fn(),
				fillStyle: '' as string,
				scale(x: number, y: number): void {
					scaleArgs.push([x, y]);
					// Store so fill() can decide pixel placement.
					(ctx as unknown as Record<string, number>)._sy = y;
				},
				translate(x: number, y: number): void {
					translateArgs.push([x, y]);
				},
				fill(_path: unknown): void {
					// Paint the first 4 rows white when Y scale is positive
					// (correct orientation); bottom 4 rows otherwise (flipped).
					const sy = (ctx as unknown as Record<string, number>)._sy ?? 1;
					const startRow = sy > 0 ? 0 : SIZE - 4;
					for (let row = startRow; row < startRow + 4; row++) {
						for (let col = 0; col < SIZE; col++) {
							const i = (row * SIZE + col) * 4;
							pixels[i] = pixels[i + 1] = pixels[i + 2] = pixels[i + 3] = 255;
						}
					}
				},
				getImageData(_x: number, _y: number, w: number, h: number): ImageData {
					// jsdom doesn't provide ImageData; return a compatible plain object.
					return { data: pixels.slice(), width: w, height: h } as unknown as ImageData;
				}
			};

			return {
				width: SIZE,
				height: SIZE,
				getContext: (type: string) => (type === '2d' ? ctx : null)
			} as unknown as HTMLCanvasElement;
		});
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('returns non-null when a canvas context is available', () => {
		const result = drawIconImage(TOP_LEFT_SQUARE);
		expect(result).not.toBeNull();
	});

	it('does not apply a non-zero Y translate (which would indicate a flip)', () => {
		drawIconImage(TOP_LEFT_SQUARE);
		const hasYFlipTranslate = translateArgs.some(([, y]) => y !== 0);
		expect(hasYFlipTranslate).toBe(false);
	});

	it('uses a positive Y scale factor (no Y-axis flip)', () => {
		drawIconImage(TOP_LEFT_SQUARE);
		expect(scaleArgs.length).toBeGreaterThan(0);
		const [, sy] = scaleArgs[0];
		expect(sy).toBeGreaterThan(0);
	});

	it('fills top-row pixels, not bottom-row, for a top-anchored icon path', () => {
		const result = drawIconImage(TOP_LEFT_SQUARE);
		if (!result) return;

		// Top-left pixel (row 0) should be opaque.
		expect(result.data[3]).toBe(255);

		// Bottom-left pixel (last row) should be transparent.
		const bottomStart = (SIZE - 1) * SIZE * 4;
		expect(result.data[bottomStart + 3]).toBe(0);
	});
});

// ─── positionAlongPath ────────────────────────────────────────────────────────

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
