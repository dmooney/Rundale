import { describe, expect, it } from 'vitest';
import {
	computeParishLayout,
	mapPlatePointToViewport,
	mapPlateRectToViewport,
} from './layout';

describe('concept-faithful illustrated parish layout', () => {
	it('tracks the canonical desktop composition', () => {
		const layout = computeParishLayout(1672, 941);

		expect(layout.mode).toBe('desktop');
		expect(layout.logoCard.x / layout.width).toBeCloseTo(0, 3);
		expect(layout.logoCard.width / layout.width).toBeCloseTo(0.225, 3);
		expect(layout.nearbyRail.y / layout.height).toBeCloseTo(0.215, 3);
		expect(layout.actionStrip.x / layout.width).toBeCloseTo(0.316, 3);
		expect(layout.actionStrip.y / layout.height).toBeCloseTo(0.79, 3);
		expect(layout.intentStrip.x / layout.width).toBeCloseTo(0.237, 3);
		expect(layout.activeIntentsCard.x / layout.width).toBeCloseTo(0.782, 3);
		expect(layout.actionCells).toHaveLength(5);
		expect(layout.tabs).toHaveLength(5);
	});

	it('never stretches the approved sewn page', () => {
		for (const [width, height] of [
			[1672, 941],
			[1440, 900],
			[390, 844],
		] as const) {
			const page = computeParishLayout(width, height).notebookPage;
			expect(page.width / page.height).toBeCloseTo(440 / 620, 5);
		}
	});

	it('keeps every notebook tab inside the viewport', () => {
		for (const [width, height] of [
			[1672, 941],
			[1440, 900],
			[390, 844],
		] as const) {
			const layout = computeParishLayout(width, height);
			for (const tab of layout.tabs) {
				expect(tab.x).toBeGreaterThanOrEqual(0);
				expect(tab.x + tab.width).toBeLessThanOrEqual(width);
			}
		}
	});

	it('leaves the side tabs visibly protruding beyond the sewn page', () => {
		for (const [width, height, minimumProtrusion] of [
			[1440, 900, 50],
			[390, 844, 25],
		] as const) {
			const layout = computeParishLayout(width, height);
			const pageRight = layout.notebookPage.x + layout.notebookPage.width;
			for (const tab of layout.tabs) {
				expect(tab.x).toBeLessThan(pageRight);
				expect(tab.x + tab.width - pageRight).toBeGreaterThanOrEqual(
					minimumProtrusion,
				);
			}
		}
	});

	it('treats the canonical notebook width as page plus tabs', () => {
		const layout = computeParishLayout(1440, 900);
		const notebookRight = Math.max(
			...layout.tabs.map((tab) => tab.x + tab.width),
		);

		expect(layout.notebookPage.x / layout.width).toBeGreaterThanOrEqual(0.765);
		expect(
			(notebookRight - layout.notebookPage.x) / layout.width,
		).toBeLessThanOrEqual(0.235);
	});

	it('keeps desktop exit signage clear of the notebook', () => {
		const layout = computeParishLayout(1440, 900);

		for (const exit of layout.exitLabels) {
			expect(exit.x + exit.width).toBeLessThan(layout.notebookPage.x);
		}
	});

	it('maps plate annotations through the same cover crop as the scene', () => {
		const point = mapPlatePointToViewport(1440, 900, 1672, 941, {
			x: 0.491,
			y: 0.57,
		});

		expect(point.x).toBeCloseTo(706, -1);
		expect(point.y).toBeCloseTo(513, -1);

		const mappedRect = mapPlateRectToViewport(1440, 900, 1672, 941, {
			x: 0.128,
			y: 0.119,
			width: 0.092,
			height: 0.037,
		});
		expect(mappedRect.x).toBeCloseTo(125, 0);
		expect(mappedRect.width).toBeCloseTo(147, 0);
	});

	it('uses the dedicated vertical composition on mobile', () => {
		const layout = computeParishLayout(390, 844);

		expect(layout.mode).toBe('mobile');
		expect(layout.nearbyRail.width).toBeGreaterThan(370);
		expect(layout.notebookPage.x / layout.width).toBeGreaterThan(0.48);
		expect(layout.actionStrip.y / layout.height).toBeCloseTo(0.67, 3);
		expect(layout.intentStrip.y / layout.height).toBeCloseTo(0.765, 3);
		expect(layout.mapCard.width).toBeGreaterThanOrEqual(44);
		expect(
			layout.tabs.every((tab) => tab.width >= 42 && tab.height >= 42),
		).toBe(true);
	});

	it('matches the overlay breakpoint at exactly 760 pixels', () => {
		expect(computeParishLayout(760, 900).mode).toBe('mobile');
		expect(computeParishLayout(761, 900).mode).toBe('desktop');
	});
});
