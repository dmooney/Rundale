import { describe, expect, it } from 'vitest';
import {
	computeNotebookLayout,
	scaleForDepth,
	sortAnchorsByDepth,
} from './layout';
import type { DepthBand } from './types';

const bands: DepthBand[] = [
	{ name: 'far', min_depth: 0, max_depth: 0.35, marker_scale: 0.5 },
	{ name: 'mid', min_depth: 0.35, max_depth: 0.7, marker_scale: 0.72 },
	{ name: 'near', min_depth: 0.7, max_depth: 1, marker_scale: 0.95 },
];

describe('illustrated notebook layout', () => {
	it('places the desktop notebook regions in the concept composition', () => {
		const layout = computeNotebookLayout(1440, 900);

		expect(layout.mode).toBe('desktop');
		expect(layout.topRibbon.width).toBe(1440);
		expect(layout.nearbyStrip.x).toBeLessThan(20);
		expect(layout.notebookPage.x).toBeGreaterThan(900);
		expect(layout.actionStamps).toHaveLength(5);
		expect(layout.intentStrip.y).toBeGreaterThan(780);
		expect(layout.mapCard).not.toBeNull();
		expect(layout.activeIntentsCard).not.toBeNull();
	});

	it('keeps the mobile viewport in notebook mode without dashboard columns', () => {
		const layout = computeNotebookLayout(390, 844);

		expect(layout.mode).toBe('mobile');
		expect(layout.topRibbon.width).toBe(390);
		expect(layout.nearbyStrip.width).toBeGreaterThan(360);
		expect(layout.actionStamps).toHaveLength(5);
		expect(layout.mapCard).toBeNull();
		expect(layout.activeIntentsCard).toBeNull();
		expect(layout.intentStrip.y).toBeGreaterThan(730);
	});

	it('scales markers by depth bands and sorts back-to-front', () => {
		expect(scaleForDepth(0.1, bands)).toBe(0.5);
		expect(scaleForDepth(0.5, bands)).toBe(0.72);
		expect(scaleForDepth(0.92, bands)).toBe(0.95);

		const sorted = sortAnchorsByDepth([
			{ id: 'near', x: 0, y: 0, depth: 0.9 },
			{ id: 'far', x: 0, y: 0, depth: 0.1 },
			{ id: 'mid', x: 0, y: 0, depth: 0.5 },
		]);
		expect(sorted.map((a) => a.id)).toEqual(['far', 'mid', 'near']);
	});
});
