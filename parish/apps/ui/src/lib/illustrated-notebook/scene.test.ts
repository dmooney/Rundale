import { describe, expect, it } from 'vitest';
import type { MapData, WorldSnapshot } from '$lib/types';
import type { VisualScene } from './types';
import { NOTEBOOK_ASSET_URLS, NOTEBOOK_ASSETS } from './assets';
import { currentNotebookLocationId, selectVisualScene } from './scene';

function scene(locationId: number, plate: string): VisualScene {
	return {
		location_ids: [locationId],
		plate_asset: plate,
		written_visual_summary: plate,
		camera_hint: 'wide elevated oblique illustrated storybook game scene',
		background_generation_source: 'Generated from written description only.',
		depth_bands: [],
		anchors: {
			player: { x: 0.5, y: 0.5, depth: 0.5 },
			npcs: [],
			exits: [],
		},
	};
}

const crossroads = scene(1, 'scene-crossroads.png');
const farm = scene(9, 'scene-murphys-farm.png');
const village = scene(15, 'scene-kilteevan-village.png');

describe('illustrated notebook scene selection', () => {
	it.each([
		['The Crossroads', 1, crossroads],
		["Murphy's Farm", 9, farm],
		['Kilteevan Village', 15, village],
	])('selects the authored plate for %s', (_name, locationId, expected) => {
		expect(
			selectVisualScene([crossroads, farm, village], locationId, crossroads),
		).toBe(expected);
	});

	it('uses a neutral, identity-bearing fallback without false exit labels', () => {
		const uncovered = selectVisualScene([farm, village], 404, crossroads);
		const unresolved = selectVisualScene([farm, village], null, crossroads);
		const authoredAssets = [crossroads, farm, village].map(
			(candidate) => candidate.plate_asset,
		);

		expect(uncovered).not.toBe(crossroads);
		expect(uncovered.location_ids).toEqual([404]);
		expect(uncovered.plate_asset).toBeNull();
		expect(authoredAssets).not.toContain(uncovered.plate_asset);
		expect(uncovered.anchors.player).toBeNull();
		expect(uncovered.anchors.npcs).toEqual([]);
		expect(uncovered.anchors.exits).toEqual([]);
		expect(unresolved.location_ids).toEqual([]);
		expect(unresolved.plate_asset).toBeNull();
		expect(unresolved.anchors.player).toBeNull();
		expect(unresolved.anchors.npcs).toEqual([]);
		expect(unresolved.anchors.exits).toEqual([]);
	});

	it('prefers the authoritative world id over a stale map location', () => {
		const map = {
			player_location: '1',
			locations: [
				{
					id: '1',
					name: 'The Crossroads',
					lat: 0,
					lon: 0,
					adjacent: false,
					hops: 0,
				},
				{
					id: '15',
					name: 'Kilteevan Village',
					lat: 0,
					lon: 0,
					adjacent: false,
					hops: 0,
				},
			],
			edges: [],
			transport_label: 'on foot',
			transport_id: 'walking',
		} satisfies MapData;
		const world = {
			location_id: 15,
			location_name: 'Kilteevan Village',
		} as WorldSnapshot;

		expect(currentNotebookLocationId(map, world)).toBe(15);
	});

	it('uses a name match before the map player id for legacy snapshots', () => {
		const map = {
			player_location: '1',
			locations: [
				{
					id: '1',
					name: 'The Crossroads',
					lat: 0,
					lon: 0,
					adjacent: false,
					hops: 0,
				},
				{
					id: '15',
					name: 'Kilteevan Village',
					lat: 0,
					lon: 0,
					adjacent: false,
					hops: 0,
				},
			],
			edges: [],
			transport_label: 'on foot',
			transport_id: 'walking',
		} satisfies MapData;
		const legacyWorld = {
			location_name: 'Kilteevan Village',
		} as WorldSnapshot;

		expect(currentNotebookLocationId(map, legacyWorld)).toBe(15);
	});

	it('changes authored scene identity when canonical movement changes location', () => {
		const staleMap = {
			player_location: '1',
			locations: [],
			edges: [],
			transport_label: 'on foot',
			transport_id: 'walking',
		} satisfies MapData;
		const scenes = [crossroads, farm, village];
		const before = selectVisualScene(
			scenes,
			currentNotebookLocationId(staleMap, {
				location_id: 1,
				location_name: 'The Crossroads',
			} as WorldSnapshot),
			crossroads,
		);
		const after = selectVisualScene(
			scenes,
			currentNotebookLocationId(staleMap, {
				location_id: 15,
				location_name: 'Kilteevan Village',
			} as WorldSnapshot),
			crossroads,
		);

		expect(before.plate_asset).toBe('scene-crossroads.png');
		expect(after.plate_asset).toBe('scene-kilteevan-village.png');
		expect(after).not.toBe(before);
	});

	it('preloads every authored harness scene plate', () => {
		expect(NOTEBOOK_ASSET_URLS).toEqual(
			expect.arrayContaining(Object.values(NOTEBOOK_ASSETS.scenePlates)),
		);
	});
});
