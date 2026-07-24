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

	it('falls back instead of retaining a stale previous-location plate', () => {
		expect(selectVisualScene([farm, village], 404, crossroads)).toBe(
			crossroads,
		);
		expect(selectVisualScene([farm, village], null, crossroads)).toBe(
			crossroads,
		);
	});

	it('resolves the canonical numeric map location and a name-based resync', () => {
		const map = {
			player_location: '15',
			locations: [
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
			location_name: 'Kilteevan Village',
		} as WorldSnapshot;

		expect(currentNotebookLocationId(map, world)).toBe(15);
		expect(
			currentNotebookLocationId(
				{ ...map, player_location: 'kilteevan' },
				world,
			),
		).toBe(15);
	});

	it('preloads every authored harness scene plate', () => {
		expect(NOTEBOOK_ASSET_URLS).toEqual(
			expect.arrayContaining(Object.values(NOTEBOOK_ASSETS.scenePlates)),
		);
	});
});
