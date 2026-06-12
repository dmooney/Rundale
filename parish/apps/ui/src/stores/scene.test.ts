import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import {
	sceneState,
	travelDimming,
	cacheScene,
	getCachedPlateUrl,
	getCachedSpriteUrl,
	hasCachedScene,
	clearSceneCache,
} from './scene';
import type { SceneState } from '$lib/types';

// ── Fixtures ─────────────────────────────────────────────────────────────────

function makeScene(
	locationId: number,
	variant: string,
	overrides: Partial<SceneState> = {},
): SceneState {
	return {
		schema_version: 1,
		location_id: locationId,
		plate_url: `data:image/png;base64,plate_${locationId}_${variant}`,
		variant,
		indoor: false,
		hotspots: [],
		npcs: [
			{
				id: 1,
				display_name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				introduced: true,
				mood_emoji: '😊',
				sprite_url: `data:image/png;base64,sprite_1`,
				x: 48,
				y: 55,
				scale: 1.0,
				flip: false,
			},
		],
		overflow_npcs: [],
		...overrides,
	};
}

// ── sceneState store ─────────────────────────────────────────────────────────

describe('sceneState store', () => {
	beforeEach(() => {
		sceneState.set(null);
	});

	it('starts as null', () => {
		expect(get(sceneState)).toBeNull();
	});

	it('can be set to a SceneState value', () => {
		const scene = makeScene(2, 'day');
		sceneState.set(scene);
		expect(get(sceneState)).toEqual(scene);
	});

	it('can be reset to null', () => {
		sceneState.set(makeScene(2, 'day'));
		sceneState.set(null);
		expect(get(sceneState)).toBeNull();
	});
});

// ── travelDimming store ───────────────────────────────────────────────────────

describe('travelDimming store', () => {
	beforeEach(() => {
		travelDimming.set(false);
	});

	it('starts as false', () => {
		expect(get(travelDimming)).toBe(false);
	});

	it('can be set to true', () => {
		travelDimming.set(true);
		expect(get(travelDimming)).toBe(true);
	});

	it('can be cleared back to false', () => {
		travelDimming.set(true);
		travelDimming.set(false);
		expect(get(travelDimming)).toBe(false);
	});
});

// ── cacheScene / hasCachedScene / getCachedPlateUrl ──────────────────────────

describe('scene cache', () => {
	beforeEach(() => {
		clearSceneCache();
	});

	it('hasCachedScene returns false before caching', () => {
		expect(hasCachedScene(2, 'day')).toBe(false);
	});

	it('cacheScene + hasCachedScene hit', () => {
		cacheScene(makeScene(2, 'day'));
		expect(hasCachedScene(2, 'day')).toBe(true);
	});

	it('getCachedPlateUrl returns the cached plate URL', () => {
		const scene = makeScene(2, 'day');
		cacheScene(scene);
		expect(getCachedPlateUrl(2, 'day')).toBe(scene.plate_url);
	});

	it('getCachedPlateUrl returns undefined on a miss', () => {
		expect(getCachedPlateUrl(99, 'day')).toBeUndefined();
	});

	it('getCachedSpriteUrl returns the sprite URL for a cached NPC', () => {
		cacheScene(makeScene(2, 'day'));
		expect(getCachedSpriteUrl(2, 'day', 1)).toBe(
			'data:image/png;base64,sprite_1',
		);
	});

	it('getCachedSpriteUrl returns undefined for an NPC not in the scene', () => {
		cacheScene(makeScene(2, 'day'));
		expect(getCachedSpriteUrl(2, 'day', 999)).toBeUndefined();
	});

	it('treats different variants as separate cache entries', () => {
		cacheScene(makeScene(2, 'day'));
		expect(hasCachedScene(2, 'night')).toBe(false);
	});

	it('treats different location ids as separate cache entries', () => {
		cacheScene(makeScene(2, 'day'));
		expect(hasCachedScene(3, 'day')).toBe(false);
	});

	it('cacheScene updates an existing entry without losing sprite data', () => {
		cacheScene(makeScene(2, 'day'));
		const updated = makeScene(2, 'day', {
			plate_url: 'data:image/png;base64,new_plate',
		});
		cacheScene(updated);
		expect(getCachedPlateUrl(2, 'day')).toBe('data:image/png;base64,new_plate');
		// sprite should still be present from the first cache call
		expect(getCachedSpriteUrl(2, 'day', 1)).toBe(
			'data:image/png;base64,sprite_1',
		);
	});

	it('clearSceneCache removes all entries', () => {
		cacheScene(makeScene(1, 'day'));
		cacheScene(makeScene(2, 'night'));
		clearSceneCache();
		expect(hasCachedScene(1, 'day')).toBe(false);
		expect(hasCachedScene(2, 'night')).toBe(false);
	});
});
