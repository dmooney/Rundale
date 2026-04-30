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

	// ── Regression #1: location icons (issue #309) ──────────────────────────────
	// The location-circles symbol layer must use the 'icon-image' expression that
	// resolves to 'icon-<name>' — matching the keys registerLocationIcons() uses
	// when it calls map.addImage('icon-<name>', …) in the controller.
	it('location-circles icon-image expression matches the icon-<name> convention', () => {
		const style = buildStyle('full', THEME, osm());
		const icons = style.layers.find((l) => l.id === 'location-circles');
		expect(icons).toBeDefined();
		expect(icons?.type).toBe('symbol');
		const layout = (icons as { layout: Record<string, unknown> }).layout;
		expect(layout['icon-image']).toEqual(['concat', 'icon-', ['get', 'icon']]);
	});

	// ── Regression #2: travel edge highlighting (issue #309) ────────────────────
	// The edges-solid layer must honour the traversing property for both line-color
	// (accent vs border) and line-width (wider when traversing).
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
		// Traversing edges should also be more opaque than normal ones.
		const opacity = (solid as { paint: { 'line-opacity': unknown } }).paint['line-opacity'];
		expect(opacity).toEqual(['case', ['get', 'traversing'], 1, 0.85]);
	});

	// ── Regression #3: glow filter for lit/player locations (issue #309) ────────
	// The location-glow circle layer approximates the old SVG feColorMatrix glow
	// via circle-blur. Lit locations must get a non-zero blur; plain ones get 0.
	it('location-glow uses circle-blur to approximate lit/player glow', () => {
		const style = buildStyle('full', THEME, osm());
		const glow = style.layers.find((l) => l.id === 'location-glow');
		expect(glow).toBeDefined();
		const paint = (glow as { paint: Record<string, unknown> }).paint;
		// circle-blur must be a case expression that gives non-zero blur to isPlayer
		// and lit locations while hiding plain ones.
		const blur = paint['circle-blur'];
		expect(Array.isArray(blur)).toBe(true);
		const blurExpr = blur as unknown[];
		expect(blurExpr[0]).toBe('case');
		// isPlayer case (index 1-2) and lit case (index 3-4) must have positive blur.
		expect(typeof blurExpr[2]).toBe('number');
		expect(blurExpr[2] as number).toBeGreaterThan(0);
		expect(typeof blurExpr[4]).toBe('number');
		expect(blurExpr[4] as number).toBeGreaterThan(0);
		// Default (last value) must be 0 — plain unlit locations have no glow.
		expect(blurExpr[blurExpr.length - 1]).toBe(0);
		// Plain locations must have 0 circle-opacity (hidden glow circle).
		const opacity = paint['circle-opacity'];
		expect(opacity).toEqual([
			'case',
			['any', ['get', 'isPlayer'], ['get', 'lit']],
			1,
			0
		]);
	});
});
