import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import { computeNotebookLayout } from './layout';
import {
	computeNearbyPortraitPlacements,
	computeNpcMarkerHitRect,
	loadNotebookRendererAssets,
} from './renderer';
import type { NotebookRect, VisualScenesFile } from './types';

const authoredScenes = JSON.parse(
	readFileSync(
		resolve(process.cwd(), 'static/rundale/notebook-ui/visual-scenes.json'),
		'utf8',
	),
) as VisualScenesFile;

function overlaps(left: NotebookRect, right: NotebookRect): boolean {
	return (
		left.x < right.x + right.width &&
		left.x + left.width > right.x &&
		left.y < right.y + right.height &&
		left.y + left.height > right.y
	);
}

describe('illustrated notebook renderer geometry', () => {
	it.each([
		[1200, 800],
		[1440, 900],
		[760, 600],
		[667, 375],
		[390, 844],
		[320, 568],
	] as const)(
		'keeps authored NPC portrait and scene-marker hit areas in bounds and disjoint at %ix%i',
		(width, height) => {
			const scene = authoredScenes.scenes.find((candidate) =>
				candidate.location_ids.includes(1),
			);
			if (!scene) throw new Error('authored crossroads scene is missing');
			expect(scene.anchors.npcs).toHaveLength(4);

			const layout = computeNotebookLayout(width, height);
			const placements = computeNearbyPortraitPlacements(
				layout,
				scene.anchors.npcs.length,
			);
			expect(placements).toHaveLength(4);

			for (const { frameRect } of placements) {
				expect(frameRect.x).toBeGreaterThanOrEqual(0);
				expect(frameRect.y).toBeGreaterThanOrEqual(0);
				expect(frameRect.x + frameRect.width).toBeLessThanOrEqual(width);
				expect(frameRect.y + frameRect.height).toBeLessThanOrEqual(height);
				expect(frameRect.x).toBeGreaterThanOrEqual(layout.nearbyStrip.x);
				expect(frameRect.y).toBeGreaterThanOrEqual(layout.nearbyStrip.y);
				expect(frameRect.x + frameRect.width).toBeLessThanOrEqual(
					layout.nearbyStrip.x + layout.nearbyStrip.width,
				);
				expect(frameRect.y + frameRect.height).toBeLessThanOrEqual(
					layout.nearbyStrip.y + layout.nearbyStrip.height,
				);
			}

			for (let left = 0; left < placements.length; left += 1) {
				for (let right = left + 1; right < placements.length; right += 1) {
					expect(
						overlaps(placements[left].frameRect, placements[right].frameRect),
					).toBe(false);
				}
			}

			const markerRects = scene.anchors.npcs.map((anchor) =>
				computeNpcMarkerHitRect(layout, anchor),
			);
			for (const markerRect of markerRects) {
				expect(markerRect.x).toBeGreaterThanOrEqual(0);
				expect(markerRect.y).toBeGreaterThanOrEqual(0);
				expect(markerRect.x + markerRect.width).toBeLessThanOrEqual(width);
				expect(markerRect.y + markerRect.height).toBeLessThanOrEqual(height);
			}
			for (let left = 0; left < markerRects.length; left += 1) {
				for (let right = left + 1; right < markerRects.length; right += 1) {
					expect(overlaps(markerRects[left], markerRects[right])).toBe(false);
				}
			}
		},
	);
});

describe('illustrated notebook renderer asset availability', () => {
	it('degrades a rejected global preload to assetless neutral paper', async () => {
		const loadScenes = vi.fn(async () => authoredScenes);
		const assets = await loadNotebookRendererAssets(async () => {
			throw new Error('preload failed');
		}, loadScenes);

		expect(loadScenes).not.toHaveBeenCalled();
		expect(assets.degraded).toBe(true);
		expect(assets.textures.size).toBe(0);
		expect(assets.scenes).toHaveLength(1);
		expect(assets.scenes[0].plate_asset).toBeNull();
		expect(assets.scenes[0].anchors.player).toBeNull();
		expect(assets.scenes[0].anchors.npcs).toEqual([]);
		expect(assets.scenes[0].anchors.exits).toEqual([]);
	});
});
