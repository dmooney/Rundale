import { writable } from 'svelte/store';
import type { SceneState } from '$lib/types';

/**
 * The current scene state as returned by `getSceneState()`.
 *
 * `null` means either:
 *   - the `diorama` flag is off, or
 *   - the player is at a location with no scene defined.
 *
 * In both cases `+page.svelte` renders the existing text+map layout unchanged.
 */
export const sceneState = writable<SceneState | null>(null);

/**
 * True while the player is in transit between locations (travel-start event
 * fired, arrival world-update not yet received). `ScenePlate` reads this to
 * apply a CSS dim effect on the departing plate.
 */
export const travelDimming = writable<boolean>(false);

// ── Tauri data-URL cache ──────────────────────────────────────────────────────
//
// In Tauri mode, plate_url and sprite_url are data URLs (sometimes hundreds of
// KB). Caching by (location_id, variant) means identical scenes fetched on
// consecutive world-updates (e.g. an NPC moves within the same location) don't
// re-decode the same asset. Web-mode URLs are plain HTTP strings that the
// browser caches natively; the cache here adds no harm but is rarely populated.

type CacheKey = string; // `${location_id}:${variant}`

interface CachedScene {
	plate_url: string;
	/** sprite_url keyed by NPC id */
	sprite_urls: Map<number, string>;
}

const sceneCache = new Map<CacheKey, CachedScene>();

function cacheKey(locationId: number, variant: string): CacheKey {
	return `${locationId}:${variant}`;
}

/**
 * Look up a cached plate URL for this (location_id, variant) pair.
 * Returns `undefined` on a miss.
 */
export function getCachedPlateUrl(
	locationId: number,
	variant: string,
): string | undefined {
	return sceneCache.get(cacheKey(locationId, variant))?.plate_url;
}

/**
 * Look up a cached sprite URL for a given NPC id within this scene.
 * Returns `undefined` on a miss.
 */
export function getCachedSpriteUrl(
	locationId: number,
	variant: string,
	npcId: number,
): string | undefined {
	return sceneCache.get(cacheKey(locationId, variant))?.sprite_urls.get(npcId);
}

/**
 * Store (or update) the plate and sprite URLs for a scene in the cache.
 * Called by the page controller after a successful `getSceneState()` fetch.
 */
export function cacheScene(state: SceneState): void {
	const key = cacheKey(state.location_id, state.variant);
	let entry = sceneCache.get(key);
	if (!entry) {
		entry = { plate_url: state.plate_url, sprite_urls: new Map() };
		sceneCache.set(key, entry);
	} else {
		entry.plate_url = state.plate_url;
	}
	for (const npc of state.npcs) {
		entry.sprite_urls.set(npc.id, npc.sprite_url);
	}
}

/**
 * Returns true if the cache already has a plate URL for this
 * (location_id, variant) pair — used by tests to verify cache hits.
 */
export function hasCachedScene(locationId: number, variant: string): boolean {
	return sceneCache.has(cacheKey(locationId, variant));
}

/** Clears the entire scene cache (test helper). */
export function clearSceneCache(): void {
	sceneCache.clear();
}
