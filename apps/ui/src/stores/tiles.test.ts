import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import type { UiConfig, TileSource } from '$lib/types';

function osm(): TileSource {
	return {
		id: 'osm',
		label: 'OpenStreetMap',
		url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
		tile_size: 256,
		minzoom: 0,
		maxzoom: 19,
		attribution: '© OSM',
		raster_saturation: -0.4,
		raster_opacity: 0.85,
		tms: false
	};
}

function historic(): TileSource {
	return {
		id: 'historic-6inch',
		label: 'Historic 6"',
		url: 'https://mapseries-tilesets.s3.amazonaws.com/ireland_6inch/{z}/{x}/{y}.jpg',
		tile_size: 256,
		minzoom: 0,
		maxzoom: 15,
		attribution: 'NLS',
		raster_saturation: 0.0,
		raster_opacity: 1.0,
		tms: false
	};
}

function cfg(active = 'osm', sources = [osm(), historic()]): UiConfig {
	return {
		hints_label: '',
		default_accent: '',
		splash_text: '',
		active_tile_source: active,
		tile_sources: sources
	};
}

// Re-import fresh each test so the module-level singleton state is reset.
async function freshStore() {
	vi.resetModules();
	return await import('./tiles');
}

describe('tiles store', () => {
	beforeEach(() => {
		localStorage.clear();
	});

	it('initFromUiConfig seeds registry and respects backend default', async () => {
		const { tiles, currentTileSource } = await freshStore();
		tiles.initFromUiConfig(cfg('osm'));
		const state = get(tiles);
		expect(state.activeId).toBe('osm');
		expect(state.sources.size).toBe(2);
		expect(currentTileSource(state)?.id).toBe('osm');
	});

	it('setActiveId switches the active source', async () => {
		const { tiles } = await freshStore();
		tiles.initFromUiConfig(cfg('osm'));
		tiles.setActiveId('historic-6inch');
		expect(get(tiles).activeId).toBe('historic-6inch');
	});

	it('setActiveId ignores unknown ids', async () => {
		const { tiles } = await freshStore();
		tiles.initFromUiConfig(cfg('osm'));
		tiles.setActiveId('bogus');
		expect(get(tiles).activeId).toBe('osm');
	});

	it('setActiveId persists choice to localStorage', async () => {
		const { tiles } = await freshStore();
		tiles.initFromUiConfig(cfg('osm'));
		tiles.setActiveId('historic-6inch');
		expect(localStorage.getItem('parish.tile-source')).toBe('historic-6inch');
	});

	it('initFromUiConfig prefers localStorage over backend default', async () => {
		localStorage.setItem('parish.tile-source', 'historic-6inch');
		const { tiles } = await freshStore();
		tiles.initFromUiConfig(cfg('osm'));
		expect(get(tiles).activeId).toBe('historic-6inch');
	});

	it('initFromUiConfig falls back to backend default when localStorage has unknown id', async () => {
		localStorage.setItem('parish.tile-source', 'vanished-source');
		const { tiles } = await freshStore();
		tiles.initFromUiConfig(cfg('osm'));
		expect(get(tiles).activeId).toBe('osm');
	});
});
