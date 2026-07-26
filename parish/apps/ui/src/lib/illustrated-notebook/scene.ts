import type { MapData, WorldSnapshot } from '$lib/types';
import type { VisualScene } from './types';

export function currentNotebookLocationId(
	map: MapData | null,
	world: WorldSnapshot | null,
): number | null {
	const authoritative = world?.location_id;
	if (
		typeof authoritative === 'number' &&
		Number.isSafeInteger(authoritative) &&
		authoritative > 0
	) {
		return authoritative;
	}

	const byName = map?.locations.find(
		(location) =>
			location.name.trim().toLocaleLowerCase() ===
			world?.location_name.trim().toLocaleLowerCase(),
	);
	if (byName) {
		const resolved = Number(byName.id);
		if (Number.isSafeInteger(resolved)) return resolved;
	}

	// Legacy snapshots predating `location_id` can still use the map as a
	// best-effort fallback. New snapshots never reach this independently
	// refreshed (and therefore potentially stale) seam.
	const current = map?.player_location?.trim();
	const direct = current ? Number(current) : Number.NaN;
	return Number.isSafeInteger(direct) ? direct : null;
}

export function selectVisualScene(
	scenes: VisualScene[],
	locationId: number | null,
	fallback: VisualScene,
): VisualScene {
	const authored =
		locationId === null
			? null
			: scenes.find((scene) => scene.location_ids.includes(locationId));
	if (authored) return authored;

	// An uncovered location must not borrow any authored geography from the
	// fallback scene. Retain only its generic rendering metadata; the null
	// plate/anchors instruct the renderer to draw code-native paper with no
	// place-specific bitmap, actors, or exit labels.
	return {
		...fallback,
		location_ids: locationId === null ? [] : [locationId],
		plate_asset: null,
		written_visual_summary:
			'No location-specific illustrated plate has been authored for this place.',
		anchors: {
			player: null,
			npcs: [],
			exits: [],
		},
	};
}
