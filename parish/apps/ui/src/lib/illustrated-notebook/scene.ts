import type { MapData, WorldSnapshot } from '$lib/types';
import type { VisualScene } from './types';

export function currentNotebookLocationId(
	map: MapData | null,
	world: WorldSnapshot | null,
): number | null {
	const current = map?.player_location?.trim();
	const direct = current ? Number(current) : Number.NaN;
	if (Number.isSafeInteger(direct)) return direct;

	const byName = map?.locations.find(
		(location) =>
			location.name.trim().toLocaleLowerCase() ===
			world?.location_name.trim().toLocaleLowerCase(),
	);
	if (!byName) return null;
	const resolved = Number(byName.id);
	return Number.isSafeInteger(resolved) ? resolved : null;
}

export function selectVisualScene(
	scenes: VisualScene[],
	locationId: number | null,
	fallback: VisualScene,
): VisualScene {
	if (locationId === null) return fallback;
	return (
		scenes.find((scene) => scene.location_ids.includes(locationId)) ?? fallback
	);
}
