/**
 * Map tile source selector store.
 *
 * Mirrors the shape of `stores/theme.ts` — holds the full registry of
 * tile sources (sent by the backend via `getUiConfig`) and the id of
 * the currently-active source. The full map's controller subscribes to
 * this store and swaps MapLibre's base raster when the active id changes.
 *
 * The backend is the source of truth — `/tiles <id>` emits a
 * `tiles-switch` event that drives this store (via `onTilesSwitch` in
 * `lib/ipc.ts`). localStorage just lets a soft-reload keep the user's
 * last choice until `getUiConfig()` resolves.
 */

import { writable } from 'svelte/store';
import type { TileSource, UiConfig } from '$lib/types';

const STORAGE_KEY = 'parish.tile-source';

function loadActiveIdFromStorage(): string | null {
	try {
		return typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;
	} catch {
		return null;
	}
}

function saveActiveIdToStorage(id: string): void {
	try {
		if (typeof localStorage !== 'undefined') localStorage.setItem(STORAGE_KEY, id);
	} catch {
		// ignore quota / disabled-storage errors
	}
}

interface TilesState {
	/** Id of the currently-active tile source. Empty string = none. */
	activeId: string;
	/** Full registry keyed by id. Populated once from `UiConfig`. */
	sources: Map<string, TileSource>;
}

function createTilesStore() {
	const initial: TilesState = {
		activeId: loadActiveIdFromStorage() ?? '',
		sources: new Map()
	};
	const { subscribe, update } = writable<TilesState>(initial);

	/**
	 * Seeds the registry from `getUiConfig()`. Called once at app boot.
	 * If localStorage already has a valid active id from a prior session,
	 * prefers it over the backend default (the backend default is only
	 * used when the client has no saved choice).
	 */
	function initFromUiConfig(cfg: UiConfig) {
		const map = new Map<string, TileSource>();
		for (const src of cfg.tile_sources) map.set(src.id, src);
		const saved = loadActiveIdFromStorage();
		const activeId = saved && map.has(saved) ? saved : cfg.active_tile_source;
		update((s) => ({ ...s, sources: map, activeId }));
	}

	/** Switches to the named source if it exists in the registry. */
	function setActiveId(id: string) {
		update((s) => {
			if (!s.sources.has(id)) return s;
			saveActiveIdToStorage(id);
			return { ...s, activeId: id };
		});
	}

	return { subscribe, initFromUiConfig, setActiveId };
}

export const tiles = createTilesStore();

/** Reads the current active `TileSource` from a `TilesState` value. */
export function currentTileSource(state: TilesState): TileSource | undefined {
	return state.sources.get(state.activeId);
}

export type { TilesState };
