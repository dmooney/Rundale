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

function overlaps(
	a: { x: number; y: number; width: number; height: number },
	b: { x: number; y: number; width: number; height: number },
): boolean {
	return (
		a.x < b.x + b.width &&
		a.x + a.width > b.x &&
		a.y < b.y + b.height &&
		a.y + a.height > b.y
	);
}

function assertSupportedRegionsDoNotOverlap(width: number, height: number) {
	const layout = computeNotebookLayout(width, height);
	const regions = [
		['top ribbon', layout.topRibbon],
		['nearby strip', layout.nearbyStrip],
		['notebook page', layout.notebookPage],
		['live chronicle', layout.liveChronicle],
		['intent strip', layout.intentStrip],
		...layout.tabs.map((tab, index) => [`notebook tab ${index}`, tab] as const),
		...layout.actionStamps.map(
			(stamp, index) => [`action stamp ${index}`, stamp] as const,
		),
		...(layout.mapCard ? ([['map card', layout.mapCard]] as const) : []),
		...(layout.timeCard ? ([['time card', layout.timeCard]] as const) : []),
		...(layout.activeIntentsCard
			? ([['active task', layout.activeIntentsCard]] as const)
			: []),
	] as Array<
		readonly [string, { x: number; y: number; width: number; height: number }]
	>;

	for (const [name, region] of regions) {
		expect(
			region.x,
			`${name} starts outside ${width}x${height}`,
		).toBeGreaterThanOrEqual(0);
		expect(
			region.y,
			`${name} starts outside ${width}x${height}`,
		).toBeGreaterThanOrEqual(0);
		expect(
			region.x + region.width,
			`${name} exceeds the right edge of ${width}x${height}`,
		).toBeLessThanOrEqual(width);
		expect(
			region.y + region.height,
			`${name} exceeds the bottom edge of ${width}x${height}`,
		).toBeLessThanOrEqual(height);
	}

	for (let left = 0; left < regions.length; left += 1) {
		for (let right = left + 1; right < regions.length; right += 1) {
			const [leftName, leftRect] = regions[left];
			const [rightName, rightRect] = regions[right];
			// Side tabs are deliberately attached over the notebook page edge.
			if (
				(leftName === 'notebook page' &&
					rightName.startsWith('notebook tab')) ||
				(rightName === 'notebook page' && leftName.startsWith('notebook tab'))
			) {
				continue;
			}
			expect(
				overlaps(leftRect, rightRect),
				`${leftName} overlaps ${rightName} at ${width}x${height}`,
			).toBe(false);
		}
	}
}

describe('illustrated notebook layout', () => {
	it('places the desktop notebook regions in the concept composition', () => {
		const layout = computeNotebookLayout(1440, 900);

		expect(layout.mode).toBe('desktop');
		expect(layout.topRibbon.width).toBe(1440);
		expect(layout.nearbyStrip.x).toBeLessThan(20);
		expect(layout.notebookPage.x).toBeGreaterThan(900);
		expect(layout.actionStamps).toHaveLength(5);
		expect(layout.liveChronicle.width).toBeGreaterThan(500);
		expect(layout.intentStrip.y).toBeGreaterThan(780);
		expect(layout.mapCard).not.toBeNull();
		expect(layout.activeIntentsCard).not.toBeNull();
		expect(layout.activeIntentsCard).toEqual({
			x: 1124,
			y: 794,
			width: 306,
			height: 104,
		});
	});

	it('keeps a compact active-task control clear of mobile notebook controls', () => {
		const layout = computeNotebookLayout(390, 844);
		const activeTask = layout.activeIntentsCard;
		if (!activeTask) throw new Error('mobile active-task control is missing');

		expect(layout.mode).toBe('mobile');
		expect(layout.topRibbon.width).toBe(390);
		expect(layout.nearbyStrip.width).toBeGreaterThan(360);
		expect(layout.actionStamps).toHaveLength(5);
		expect(layout.mapCard).toBeNull();
		expect(activeTask.width).toBeGreaterThanOrEqual(180);
		expect(activeTask.height).toBeLessThanOrEqual(64);
		expect(activeTask.y).toBeGreaterThanOrEqual(
			layout.nearbyStrip.y + layout.nearbyStrip.height + 8,
		);
		expect(activeTask.x + activeTask.width).toBeLessThanOrEqual(
			layout.notebookPage.x - 8,
		);
		expect(overlaps(activeTask, layout.nearbyStrip)).toBe(false);
		expect(overlaps(activeTask, layout.notebookPage)).toBe(false);
		expect(
			layout.actionStamps.some((stamp) => overlaps(activeTask, stamp)),
		).toBe(false);
		expect(overlaps(activeTask, layout.intentStrip)).toBe(false);
		expect(overlaps(layout.liveChronicle, layout.notebookPage)).toBe(false);
		expect(layout.intentStrip.y).toBeGreaterThan(730);
	});

	it('keeps the compact task control usable on a narrow, short phone', () => {
		const layout = computeNotebookLayout(320, 568);
		const activeTask = layout.activeIntentsCard;
		if (!activeTask) throw new Error('mobile active-task control is missing');

		expect(activeTask.width).toBeGreaterThan(140);
		expect(activeTask.height).toBeGreaterThanOrEqual(44);
		expect(activeTask.x).toBeGreaterThanOrEqual(0);
		expect(activeTask.x + activeTask.width).toBeLessThanOrEqual(320);
		expect(activeTask.y + activeTask.height).toBeLessThan(
			Math.min(...layout.actionStamps.map((stamp) => stamp.y)),
		);
		expect(overlaps(activeTask, layout.nearbyStrip)).toBe(false);
		expect(overlaps(activeTask, layout.notebookPage)).toBe(false);
		expect(overlaps(layout.liveChronicle, layout.notebookPage)).toBe(false);
		expect(overlaps(activeTask, layout.intentStrip)).toBe(false);
	});

	it.each([
		[760, 600, 'mobile'],
		[1200, 800, 'desktop'],
		[667, 375, 'mobile'],
		[390, 844, 'mobile'],
		[320, 568, 'mobile'],
		[1440, 900, 'desktop'],
	] as const)(
		'keeps supported regions pairwise clear at %ix%i',
		(width, height, expectedMode) => {
			const layout = computeNotebookLayout(width, height);
			expect(layout.mode).toBe(expectedMode);
			if (width === 667 && height === 375) {
				expect(layout.tabs).toEqual([]);
			}
			assertSupportedRegionsDoNotOverlap(width, height);
		},
	);

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
