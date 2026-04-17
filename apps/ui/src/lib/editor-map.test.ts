import { describe, expect, it } from 'vitest';

import type { LocationData, RelativeRef } from './editor-types';
import {
	applyDraggedCoordinates,
	buildEditorMapData,
	getEditorMapCenter,
	normalizeLocationCaches,
	offsetLatLon
} from './editor-map';

function makeLocation({
	id,
	name = `Location ${id}`,
	lat,
	lon,
	connections = [],
	relative_to = null
}: {
	id: number;
	name?: string;
	lat: number;
	lon: number;
	connections?: Array<{ target: number; path_description: string }>;
	relative_to?: RelativeRef | null;
}): LocationData {
	return {
		id,
		name,
		description_template: '',
		indoor: false,
		public: true,
		connections,
		lat,
		lon,
		associated_npcs: [],
		aliases: [],
		geo_kind: 'fictional',
		relative_to,
		geo_source: null
	};
}

describe('buildEditorMapData', () => {
	it('moves connected edges along with the dragged preview point', () => {
		const locations = [
			makeLocation({
				id: 1,
				lat: 53.5,
				lon: -8.1,
				connections: [{ target: 2, path_description: 'lane' }]
			}),
			makeLocation({
				id: 2,
				lat: 53.51,
				lon: -8.11,
				connections: [{ target: 1, path_description: 'lane' }]
			})
		];

		const preview = { id: 1, lat: 53.6, lon: -8.2 };
		const { features, edgeFeatures } = buildEditorMapData(locations, 1, preview);
		expect(features[0].geometry.coordinates).toEqual([preview.lon, preview.lat]);
		expect(edgeFeatures[0].geometry.coordinates[0]).toEqual([preview.lon, preview.lat]);
	});

	it('re-resolves relative locations from a dragged anchor preview', () => {
		const anchor = makeLocation({ id: 1, lat: 53.5, lon: -8.1 });
		const child = makeLocation({
			id: 2,
			lat: 0,
			lon: 0,
			relative_to: { anchor: 1, dnorth_m: 100, deast_m: 50 }
		});
		const preview = { id: 1, lat: 53.6, lon: -8.2 };
		const expected = offsetLatLon(preview.lat, preview.lon, 100, 50);

		const { features } = buildEditorMapData([anchor, child], 1, preview);
		expect(features[1].geometry.coordinates[0]).toBeCloseTo(expected.lon);
		expect(features[1].geometry.coordinates[1]).toBeCloseTo(expected.lat);
	});
});

describe('relative drag helpers', () => {
	it('updates relative offsets when dragging a relative location', () => {
		const anchor = makeLocation({ id: 1, lat: 53.5, lon: -8.1 });
		const child = makeLocation({
			id: 2,
			lat: 0,
			lon: 0,
			relative_to: { anchor: 1, dnorth_m: 0, deast_m: 0 }
		});
		const draggedTo = offsetLatLon(anchor.lat, anchor.lon, 250, -125);

		const moved = applyDraggedCoordinates(child, [anchor, child], draggedTo.lat, draggedTo.lon);
		expect(moved.lat).toBeCloseTo(draggedTo.lat);
		expect(moved.lon).toBeCloseTo(draggedTo.lon);
		expect(moved.relative_to?.dnorth_m).toBeCloseTo(250, 3);
		expect(moved.relative_to?.deast_m).toBeCloseTo(-125, 3);
	});

	it('normalizes cached lat lon for relative chains before persisting', () => {
		const anchor = makeLocation({ id: 1, lat: 53.5, lon: -8.1 });
		const child = makeLocation({
			id: 2,
			lat: 0,
			lon: 0,
			relative_to: { anchor: 1, dnorth_m: 100, deast_m: 50 }
		});
		const grandchild = makeLocation({
			id: 3,
			lat: 0,
			lon: 0,
			relative_to: { anchor: 2, dnorth_m: -20, deast_m: 10 }
		});

		const normalized = normalizeLocationCaches([anchor, child, grandchild]);
		const expectedChild = offsetLatLon(anchor.lat, anchor.lon, 100, 50);
		const expectedGrandchild = offsetLatLon(expectedChild.lat, expectedChild.lon, -20, 10);

		expect(normalized[1].lat).toBeCloseTo(expectedChild.lat);
		expect(normalized[1].lon).toBeCloseTo(expectedChild.lon);
		expect(normalized[2].lat).toBeCloseTo(expectedGrandchild.lat);
		expect(normalized[2].lon).toBeCloseTo(expectedGrandchild.lon);
	});
});

describe('getEditorMapCenter', () => {
	it('returns the selected feature center when not previewing a drag', () => {
		const { features } = buildEditorMapData(
			[
				makeLocation({ id: 1, lat: 53.5, lon: -8.1 }),
				makeLocation({ id: 2, lat: 53.51, lon: -8.11 })
			],
			2
		);

		expect(getEditorMapCenter(features, 2)).toEqual([-8.11, 53.51]);
	});

	it('suppresses recentering while a drag preview is active', () => {
		const preview = { id: 1, lat: 53.6, lon: -8.2 };
		const { features } = buildEditorMapData(
			[
				makeLocation({ id: 1, lat: 53.5, lon: -8.1 }),
				makeLocation({ id: 2, lat: 53.51, lon: -8.11 })
			],
			1,
			preview
		);

		expect(getEditorMapCenter(features, 1, preview)).toBeNull();
	});
});
