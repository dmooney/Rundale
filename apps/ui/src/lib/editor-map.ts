import type { LocationData } from './editor-types';

export interface EditorMapPreview {
	id: number;
	lat: number;
	lon: number;
}

export interface EditorPointFeature {
	type: 'Feature';
	properties: {
		id: number;
		name: string;
		selected: number;
		relative: number;
	};
	geometry: {
		type: 'Point';
		coordinates: [number, number];
	};
}

export interface EditorEdgeFeature {
	type: 'Feature';
	properties: { a: number; b: number };
	geometry: { type: 'LineString'; coordinates: [number, number][] };
}

const METERS_PER_DEGREE = 111_320;

export function offsetLatLon(lat: number, lon: number, northM: number, eastM: number) {
	const dLat = northM / METERS_PER_DEGREE;
	const cosLat = Math.max(0.2, Math.cos((lat * Math.PI) / 180));
	const dLon = eastM / (METERS_PER_DEGREE * cosLat);
	return { lat: lat + dLat, lon: lon + dLon };
}

export function metersFromLatLon(anchorLat: number, anchorLon: number, lat: number, lon: number) {
	const dnorth_m = (lat - anchorLat) * METERS_PER_DEGREE;
	const cosLat = Math.max(0.2, Math.cos((anchorLat * Math.PI) / 180));
	const deast_m = (lon - anchorLon) * METERS_PER_DEGREE * cosLat;
	return { dnorth_m, deast_m };
}

export function resolveLocationCoordinates(
	locations: LocationData[],
	preview?: EditorMapPreview
): Map<number, { lat: number; lon: number }> {
	const byId = new Map(locations.map((entry) => [entry.id, entry]));
	const resolved = new Map<number, { lat: number; lon: number }>();
	const resolving = new Set<number>();

	function resolve(entry: LocationData): { lat: number; lon: number } {
		if (resolved.has(entry.id)) return resolved.get(entry.id)!;

		if (preview?.id === entry.id) {
			const coords = { lat: preview.lat, lon: preview.lon };
			resolved.set(entry.id, coords);
			return coords;
		}

		if (!entry.relative_to) {
			const coords = { lat: entry.lat, lon: entry.lon };
			resolved.set(entry.id, coords);
			return coords;
		}

		if (resolving.has(entry.id)) {
			const coords = { lat: entry.lat, lon: entry.lon };
			resolved.set(entry.id, coords);
			return coords;
		}

		const anchor = byId.get(entry.relative_to.anchor);
		if (!anchor) {
			const coords = { lat: entry.lat, lon: entry.lon };
			resolved.set(entry.id, coords);
			return coords;
		}

		resolving.add(entry.id);
		const anchorCoords = resolve(anchor);
		resolving.delete(entry.id);

		const coords = offsetLatLon(
			anchorCoords.lat,
			anchorCoords.lon,
			entry.relative_to.dnorth_m,
			entry.relative_to.deast_m
		);
		resolved.set(entry.id, coords);
		return coords;
	}

	for (const entry of locations) resolve(entry);
	return resolved;
}

export function normalizeLocationCaches(locations: LocationData[]): LocationData[] {
	const resolved = resolveLocationCoordinates(locations);
	return locations.map((entry) => {
		const coords = resolved.get(entry.id) ?? { lat: entry.lat, lon: entry.lon };
		return { ...entry, lat: coords.lat, lon: coords.lon };
	});
}

export function applyDraggedCoordinates(
	location: LocationData,
	locations: LocationData[],
	lat: number,
	lon: number
): LocationData {
	if (!location.relative_to) return { ...location, lat, lon };

	const resolved = resolveLocationCoordinates(locations);
	const anchorCoords = resolved.get(location.relative_to.anchor);
	if (!anchorCoords) return { ...location, lat, lon };

	const offsets = metersFromLatLon(anchorCoords.lat, anchorCoords.lon, lat, lon);
	return {
		...location,
		lat,
		lon,
		relative_to: {
			...location.relative_to,
			dnorth_m: offsets.dnorth_m,
			deast_m: offsets.deast_m
		}
	};
}

export function buildEditorMapData(
	locations: LocationData[],
	selectedId: number | null,
	preview?: EditorMapPreview
) {
	const resolved = resolveLocationCoordinates(locations, preview);
	const features: EditorPointFeature[] = locations.map((entry) => {
		const coords = resolved.get(entry.id) ?? { lat: entry.lat, lon: entry.lon };
		return {
			type: 'Feature',
			properties: {
				id: entry.id,
				name: entry.name,
				selected: entry.id === selectedId ? 1 : 0,
				relative: entry.relative_to ? 1 : 0
			},
			geometry: { type: 'Point', coordinates: [coords.lon, coords.lat] }
		};
	});
	const edgeFeatures: EditorEdgeFeature[] = [];
	for (const entry of locations) {
		const entryCoords = resolved.get(entry.id) ?? { lat: entry.lat, lon: entry.lon };
		for (const conn of entry.connections) {
			if (entry.id > conn.target) continue;
			const target = locations.find((loc) => loc.id === conn.target);
			if (!target) continue;
			const targetCoords = resolved.get(target.id) ?? { lat: target.lat, lon: target.lon };
			edgeFeatures.push({
				type: 'Feature',
				properties: { a: entry.id, b: target.id },
				geometry: {
					type: 'LineString',
					coordinates: [
						[entryCoords.lon, entryCoords.lat],
						[targetCoords.lon, targetCoords.lat]
					]
				}
			});
		}
	}
	return { features, edgeFeatures };
}

export function getEditorMapCenter(
	features: EditorPointFeature[],
	focusId: number | null,
	preview?: EditorMapPreview
): [number, number] | null {
	if (preview || focusId === null) return null;
	const focusFeature = features.find((feature) => feature.properties.id === focusId);
	return focusFeature?.geometry.coordinates ?? null;
}
