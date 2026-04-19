import { describe, it, expect } from 'vitest';
import {
	locationsToGeoJSON,
	edgesToGeoJSON,
	computeOffMapCounts,
	edgeKey
} from './geojson';
import type { MapData } from '$lib/types';

function buildMap(overrides: Partial<MapData> = {}): MapData {
	return {
		locations: [
			{ id: 'a', name: 'Alpha', lat: 53.59, lon: -8.15, adjacent: false, hops: 0 },
			{ id: 'b', name: 'Beta Church', lat: 53.6, lon: -8.14, adjacent: true, hops: 1 },
			{
				id: 'c',
				name: 'Gamma',
				lat: 53.61,
				lon: -8.13,
				adjacent: false,
				hops: 2,
				visited: false
			}
		],
		edges: [
			['a', 'b'],
			['b', 'c']
		],
		player_location: 'a',
		player_lat: 53.59,
		player_lon: -8.15,
		edge_traversals: [['a', 'b', 4]],
		...overrides
	};
}

describe('locationsToGeoJSON', () => {
	it('returns a FeatureCollection with one Point feature per location', () => {
		const fc = locationsToGeoJSON(buildMap());
		expect(fc.type).toBe('FeatureCollection');
		expect(fc.features).toHaveLength(3);
		expect(fc.features[0].geometry.type).toBe('Point');
	});

	it('writes [lon, lat] coordinates in GeoJSON order', () => {
		const fc = locationsToGeoJSON(buildMap());
		const first = fc.features[0];
		expect(first.geometry.coordinates[0]).toBeCloseTo(-8.15);
		expect(first.geometry.coordinates[1]).toBeCloseTo(53.59);
	});

	it('marks the player location', () => {
		const fc = locationsToGeoJSON(buildMap());
		const player = fc.features.find((f) => f.properties.id === 'a');
		const other = fc.features.find((f) => f.properties.id === 'b');
		expect(player?.properties.isPlayer).toBe(true);
		expect(other?.properties.isPlayer).toBe(false);
	});

	it('marks unvisited locations as fog-of-war', () => {
		const fc = locationsToGeoJSON(buildMap());
		const gamma = fc.features.find((f) => f.properties.id === 'c');
		expect(gamma?.properties.visited).toBe(false);
	});

	it('marks "lit" locations whose name matches the keyword patterns', () => {
		const fc = locationsToGeoJSON(buildMap());
		const lit = fc.features.find((f) => f.properties.id === 'b');
		const plain = fc.features.find((f) => f.properties.id === 'a');
		expect(lit?.properties.lit).toBe(true); // "Beta Church" matches /church/
		expect(plain?.properties.lit).toBe(false);
	});

	it('assigns a semantic icon key from the location name', () => {
		const fc = locationsToGeoJSON(buildMap());
		const church = fc.features.find((f) => f.properties.id === 'b');
		const plain = fc.features.find((f) => f.properties.id === 'a');
		expect(church?.properties.icon).toBe('church');
		expect(plain?.properties.icon).toBe('map-pin');
	});

	it('filters to the given id set when provided', () => {
		const fc = locationsToGeoJSON(buildMap(), {
			filterIds: new Set(['a', 'b'])
		});
		expect(fc.features).toHaveLength(2);
		expect(fc.features.map((f) => f.properties.id).sort()).toEqual(['a', 'b']);
	});
});

describe('edgesToGeoJSON', () => {
	it('returns a LineString feature per edge', () => {
		const fc = edgesToGeoJSON(buildMap());
		expect(fc.features).toHaveLength(2);
		expect(fc.features[0].geometry.type).toBe('LineString');
		expect(fc.features[0].geometry.coordinates).toHaveLength(2);
	});

	it('normalizes traversal weights to 0..1', () => {
		const fc = edgesToGeoJSON(buildMap());
		const ab = fc.features.find(
			(f) => f.properties.src === 'a' && f.properties.dst === 'b'
		);
		const bc = fc.features.find(
			(f) => f.properties.src === 'b' && f.properties.dst === 'c'
		);
		expect(ab?.properties.traversalWeight).toBeCloseTo(1); // 4 / 4
		expect(bc?.properties.traversalWeight).toBeCloseTo(0); // no traversals
	});

	it('flags frontier edges (touching an unvisited node)', () => {
		const fc = edgesToGeoJSON(buildMap());
		const ab = fc.features.find((f) => f.properties.src === 'a');
		const bc = fc.features.find((f) => f.properties.src === 'b');
		expect(ab?.properties.frontier).toBe(false);
		expect(bc?.properties.frontier).toBe(true); // touches unvisited 'c'
	});

	it('marks active travel path edges as traversing', () => {
		const fc = edgesToGeoJSON(buildMap(), {
			traversingEdgeKeys: new Set([edgeKey('a', 'b')])
		});
		const ab = fc.features.find((f) => f.properties.src === 'a');
		const bc = fc.features.find((f) => f.properties.src === 'b');
		expect(ab?.properties.traversing).toBe(true);
		expect(bc?.properties.traversing).toBe(false);
	});

	it('filters edges whose endpoints are not all in the visible set', () => {
		const fc = edgesToGeoJSON(buildMap(), {
			filterIds: new Set(['a', 'b'])
		});
		expect(fc.features).toHaveLength(1);
		expect(fc.features[0].properties.src).toBe('a');
	});
});

describe('computeOffMapCounts', () => {
	it('counts edges that leave the visible set', () => {
		const edges: Array<[string, string]> = [
			['a', 'b'],
			['a', 'c'],
			['b', 'c']
		];
		const visible = new Set(['a', 'b']);
		const counts = computeOffMapCounts(edges, visible);
		expect(counts.get('a')).toBe(1); // a→c leaves
		expect(counts.get('b')).toBe(1); // b→c leaves
		expect(counts.get('c')).toBeUndefined();
	});

	it('returns empty when every edge stays inside the visible set', () => {
		const edges: Array<[string, string]> = [['a', 'b']];
		const visible = new Set(['a', 'b']);
		expect(computeOffMapCounts(edges, visible).size).toBe(0);
	});
});

describe('edgeKey', () => {
	it('is canonical regardless of argument order', () => {
		expect(edgeKey('a', 'b')).toBe(edgeKey('b', 'a'));
	});
});
