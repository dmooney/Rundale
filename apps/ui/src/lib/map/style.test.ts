import { describe, it, expect } from 'vitest';
import { buildStyle, type ThemeColors } from './style';
import type { TileSource } from '$lib/types';

const THEME: ThemeColors = {
	bg: '#fafad8',
	fg: '#31240f',
	accent: '#b08531',
	panelBg: '#f5f5d3',
	border: '#cec293',
	muted: '#76663b'
};

function osm(): TileSource {
	return {
		id: 'osm',
		label: 'OpenStreetMap',
		url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
		tile_size: 256,
		minzoom: 0,
		maxzoom: 19,
		attribution: '© OpenStreetMap contributors',
		raster_saturation: -0.4,
		raster_opacity: 0.85,
		tms: false
	};
}

function tmsSource(): TileSource {
	return {
		id: 'arcgis-tms',
		label: 'ArcGIS-style y-flipped',
		url: 'https://example.test/MapServer/tile/{z}/{y}/{x}',
		tile_size: 256,
		minzoom: 0,
		maxzoom: 17,
		attribution: 'example',
		raster_saturation: 0.0,
		raster_opacity: 1.0,
		tms: true
	};
}

describe('buildStyle', () => {
	it('full map with OSM source wires URL and no TMS scheme', () => {
		const style = buildStyle('full', THEME, osm());
		const src = style.sources['map-tiles'];
		expect(src).toBeDefined();
		expect(src.type).toBe('raster');
		const raster = src as { tiles: string[]; scheme?: string; attribution: string };
		expect(raster.tiles).toEqual(['https://tile.openstreetmap.org/{z}/{x}/{y}.png']);
		expect(raster.scheme).toBeUndefined();
		expect(raster.attribution).toBe('© OpenStreetMap contributors');

		const rasterLayer = style.layers.find((l) => l.id === 'map-tiles-layer');
		expect(rasterLayer).toBeDefined();
		expect((rasterLayer as { paint: { 'raster-saturation': number } }).paint['raster-saturation']).toBe(-0.4);
		// No flat-bg layer should be present on the full map when tiles render.
		expect(style.layers.find((l) => l.id === 'background')).toBeUndefined();
	});

	it('full map with TMS source sets scheme: tms', () => {
		const style = buildStyle('full', THEME, tmsSource());
		const raster = style.sources['map-tiles'] as { scheme?: string };
		expect(raster.scheme).toBe('tms');
	});

	it('full map with empty URL falls back to flat background', () => {
		const empty: TileSource = { ...osm(), id: 'unconfigured', url: '' };
		const style = buildStyle('full', THEME, empty);
		expect(style.sources['map-tiles']).toBeUndefined();
		expect(style.layers.find((l) => l.id === 'background')).toBeDefined();
		expect(style.layers.find((l) => l.id === 'map-tiles-layer')).toBeUndefined();
	});

	it('full map with no tileSource at all falls back to flat background', () => {
		const style = buildStyle('full', THEME);
		expect(style.sources['map-tiles']).toBeUndefined();
		expect(style.layers.find((l) => l.id === 'background')).toBeDefined();
	});

	it('minimap uses raster tiles when a tile source is provided', () => {
		const style = buildStyle('minimap', THEME, osm());
		expect(style.sources['map-tiles']).toBeDefined();
		expect(style.layers.find((l) => l.id === 'background')).toBeUndefined();
		expect(style.layers.find((l) => l.id === 'map-tiles-layer')).toBeDefined();
	});

	it('minimap with no tile source falls back to flat background', () => {
		const style = buildStyle('minimap', THEME);
		expect(style.sources['map-tiles']).toBeUndefined();
		expect(style.layers.find((l) => l.id === 'background')).toBeDefined();
	});

	it('always carries empty GeoJSON sources for locations and edges', () => {
		const style = buildStyle('full', THEME, osm());
		expect(style.sources.locations.type).toBe('geojson');
		expect(style.sources.edges.type).toBe('geojson');
	});

	it('renders custom icon symbols and glow layer for locations', () => {
		const style = buildStyle('full', THEME, osm());
		const glow = style.layers.find((l) => l.id === 'location-glow');
		expect(glow).toBeDefined();
		expect(glow?.type).toBe('circle');

		const icons = style.layers.find((l) => l.id === 'location-circles');
		expect(icons).toBeDefined();
		expect(icons?.type).toBe('symbol');
	});

	it('adds traversing-edge highlight styling to solid edges', () => {
		const style = buildStyle('full', THEME, osm());
		const solid = style.layers.find((l) => l.id === 'edges-solid');
		expect(solid).toBeDefined();
		expect((solid as { paint: { 'line-color': unknown } }).paint['line-color']).toEqual([
			'case',
			['get', 'traversing'],
			THEME.accent,
			THEME.border
		]);
	});
});
