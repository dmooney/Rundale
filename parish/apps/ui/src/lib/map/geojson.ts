/**
 * GeoJSON adapters for the MapLibre-powered map views.
 *
 * Converts the `MapData` IPC payload into FeatureCollections that MapLibre's
 * GeoJSON source can consume. Feature properties carry everything the style
 * expressions in `./style.ts` read — `icon`, `isPlayer`, `adjacent`, `hops`,
 * `visited`, `lit`, and `traversals` — so styling is fully data-driven.
 */

import type { FeatureCollection, Point, LineString } from 'geojson';
import type { MapData, MapLocation } from '$lib/types';
import { getLocationIcon, type LocationIcon } from '$lib/map-icons';

/** Feature-state hint patterns for "lit" locations (glow at night / standout). */
const LIT_PATTERNS = /pub|church|house|village|town|shop|school|letter/i;

/** Properties attached to each location feature. */
export interface LocationFeatureProps {
	id: string;
	name: string;
	icon: LocationIcon;
	isPlayer: boolean;
	adjacent: boolean;
	hops: number;
	visited: boolean;
	lit: boolean;
	indoor: boolean;
	travelMinutes: number;
	/** Count of edges from this node that leave the currently-visible set. */
	offMapCount: number;
}

/** Properties attached to each edge feature. */
export interface EdgeFeatureProps {
	src: string;
	dst: string;
	/** Number of times the player has traversed this edge (>=0). */
	traversals: number;
	/** Normalized traversal weight 0..1 for line-width scaling. */
	traversalWeight: number;
	/** True when either endpoint is unvisited (fog-of-war frontier). */
	frontier: boolean;
	/** True when the edge is part of the currently active travel path. */
	traversing: boolean;
}

/**
 * Returns the count of edges from each location that leave `visibleIds`.
 * Used to show "road continues" stubs on the minimap.
 */
export function computeOffMapCounts(
	edges: ReadonlyArray<[string, string]>,
	visibleIds: ReadonlySet<string>
): Map<string, number> {
	const counts = new Map<string, number>();
	for (const [a, b] of edges) {
		if (visibleIds.has(a) && !visibleIds.has(b)) {
			counts.set(a, (counts.get(a) ?? 0) + 1);
		}
		if (visibleIds.has(b) && !visibleIds.has(a)) {
			counts.set(b, (counts.get(b) ?? 0) + 1);
		}
	}
	return counts;
}

/**
 * Converts `MapData` locations into a GeoJSON FeatureCollection of Points.
 *
 * Each feature's geometry is `[lon, lat]` and properties carry everything
 * the style expressions need. Pass `filterIds` to restrict the output to a
 * subset (used by the minimap to show only nearby locations).
 */
export function locationsToGeoJSON(
	map: MapData,
	options: {
		filterIds?: ReadonlySet<string>;
		offMapCounts?: ReadonlyMap<string, number>;
	} = {}
): FeatureCollection<Point, LocationFeatureProps> {
	const { filterIds, offMapCounts } = options;
	const locations = filterIds
		? map.locations.filter((l) => filterIds.has(l.id))
		: map.locations;

	return {
		type: 'FeatureCollection',
		features: locations.map((loc) => ({
			type: 'Feature',
			geometry: {
				type: 'Point',
				coordinates: [loc.lon, loc.lat]
			},
			properties: buildLocationProps(loc, map.player_location, offMapCounts?.get(loc.id) ?? 0)
		}))
	};
}

function buildLocationProps(
	loc: MapLocation,
	playerLocation: string,
	offMapCount: number
): LocationFeatureProps {
	const visited = loc.visited !== false;
	return {
		id: loc.id,
		name: loc.name,
		icon: getLocationIcon(loc.name),
		isPlayer: loc.id === playerLocation,
		adjacent: loc.adjacent,
		hops: loc.hops,
		visited,
		lit: visited && LIT_PATTERNS.test(loc.name),
		indoor: loc.indoor ?? false,
		travelMinutes: loc.travel_minutes ?? 0,
		offMapCount
	};
}

/**
 * Converts `MapData` edges into a GeoJSON FeatureCollection of LineStrings.
 *
 * Each feature carries `traversals`, a normalized `traversalWeight`, and a
 * `frontier` flag so the line layer in `style.ts` can style footprints and
 * fog-of-war edges purely from data.
 */
export function edgesToGeoJSON(
	map: MapData,
	options: {
		filterIds?: ReadonlySet<string>;
		traversingEdgeKeys?: ReadonlySet<string>;
	} = {}
): FeatureCollection<LineString, EdgeFeatureProps> {
	const { filterIds, traversingEdgeKeys } = options;
	const locById = new Map(map.locations.map((l) => [l.id, l]));
	const traversalsByKey = buildTraversalMap(map.edge_traversals);
	const maxTraversal = Math.max(1, ...(map.edge_traversals ?? []).map(([, , c]) => c));

	const features: FeatureCollection<LineString, EdgeFeatureProps>['features'] = [];
	for (const [a, b] of map.edges) {
		if (filterIds && !(filterIds.has(a) && filterIds.has(b))) continue;
		const srcLoc = locById.get(a);
		const dstLoc = locById.get(b);
		if (!srcLoc || !dstLoc) continue;

		const key = edgeKey(a, b);
		const traversals = traversalsByKey.get(key) ?? 0;
		const weight = traversals > 0 ? traversals / maxTraversal : 0;

		features.push({
			type: 'Feature',
			geometry: {
				type: 'LineString',
				coordinates: [
					[srcLoc.lon, srcLoc.lat],
					[dstLoc.lon, dstLoc.lat]
				]
			},
			properties: {
				src: a,
				dst: b,
				traversals,
				traversalWeight: weight,
				frontier: srcLoc.visited === false || dstLoc.visited === false,
				traversing: traversingEdgeKeys?.has(key) ?? false
			}
		});
	}

	return { type: 'FeatureCollection', features };
}

/** Canonical undirected edge key (sorted). */
export function edgeKey(a: string, b: string): string {
	return a < b ? `${a}-${b}` : `${b}-${a}`;
}

function buildTraversalMap(
	traversals: MapData['edge_traversals']
): Map<string, number> {
	const map = new Map<string, number>();
	if (!traversals) return map;
	for (const [a, b, count] of traversals) {
		map.set(edgeKey(a, b), count);
	}
	return map;
}
