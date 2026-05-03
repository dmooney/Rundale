/**
 * MapLibre style JSON factory for the Parish map views.
 *
 * Produces a `StyleSpecification` tailored to either the minimap or the
 * full-parish overlay. The style wires up:
 *
 *   - a raster tile base (when configured) or a flat panel background
 *   - an `edges` line layer with data-driven width for traversal footprints
 *   - a `locations` symbol layer with MapLibre's production-grade label
 *     placement — variable anchors, symbol sort keys, halo, and zoom-level
 *     collision handling. This is the whole point of the migration.
 *
 * Colors are pulled from CSS custom properties on `:root` at call time so
 * the MapLibre style tracks the live theme. Re-call `buildStyle()` when the
 * theme changes and pass the result to `map.setStyle()`.
 */

import type {
	StyleSpecification,
	LayerSpecification,
	RasterSourceSpecification
} from 'maplibre-gl';
import type { TileSource } from '$lib/types';

export type MapVariant = 'minimap' | 'full';

export interface ThemeColors {
	bg: string;
	fg: string;
	accent: string;
	panelBg: string;
	border: string;
	muted: string;
	/** Map editor overlay colors — vars defined in app.css (#711). */
	mapEdge: string;
	mapSelected: string;
	mapRelative: string;
	mapStroke: string;
}

/** Reads the live theme colors from CSS custom properties on `:root`. */
export function readThemeColors(root: HTMLElement = document.documentElement): ThemeColors {
	const styles = getComputedStyle(root);
	const get = (name: string, fallback: string) =>
		styles.getPropertyValue(name).trim() || fallback;
	return {
		bg: get('--color-bg', '#fafad8'),
		fg: get('--color-fg', '#31240f'),
		accent: get('--color-accent', '#b08531'),
		panelBg: get('--color-panel-bg', '#f5f5d3'),
		border: get('--color-border', '#cec293'),
		muted: get('--color-muted', '#76663b'),
		mapEdge: get('--color-map-edge', '#8f7e56'),
		mapSelected: get('--color-map-selected', '#f4cf75'),
		mapRelative: get('--color-map-relative', '#7dd7ff'),
		mapStroke: get('--color-map-stroke', '#1a140a')
	};
}

/**
 * MapLibre glyphs endpoint default — the MapLibre demo CDN, which has no SLA.
 * Override via `buildStyle`'s `glyphsUrl` parameter to point at self-hosted
 * PBFs (e.g. `/fonts/{fontstack}/{range}.pbf`) once bundled as static assets.
 * TODO: bundle Open Sans glyph PBFs as static assets to work fully offline.
 */
export const DEFAULT_GLYPHS_URL =
	'https://demotiles.maplibre.org/font/{fontstack}/{range}.pbf';

/**
 * Builds a MapLibre style spec for the given map variant and theme.
 *
 * The style has two GeoJSON sources (`locations` and `edges`) that start
 * empty — the controller populates them via `setData()` as game state
 * changes. A raster base is added beneath when `tileSource` has a URL
 * (ships with OSM by default; the `/tiles` slash command swaps this via
 * `MapController.setTileSource()`).
 *
 * Passing a `tileSource` with an empty `url` (e.g. a user-added source
 * that hasn't had its URL filled in yet) falls back to the flat-bg layer
 * with a one-shot console warning — the feature flag can stay on without
 * a live endpoint.
 */
export function buildStyle(
	variant: MapVariant,
	theme: ThemeColors,
	tileSource?: TileSource,
	glyphsUrl: string = DEFAULT_GLYPHS_URL
): StyleSpecification {
	const layers: LayerSpecification[] = [];
	const rasterSourceId = 'map-tiles';
	const hasUsableTiles = !!tileSource && tileSource.url.length > 0;

	// 1. Base layer — configured raster when available, otherwise flat background.
	if (hasUsableTiles) {
		layers.push({
			id: 'map-tiles-layer',
			type: 'raster',
			source: rasterSourceId,
			paint: {
				'raster-saturation': tileSource!.raster_saturation,
				'raster-opacity': tileSource!.raster_opacity
			}
		});
	} else {
		if (tileSource && tileSource.url.length === 0) {
			// Informational — a source was registered without a URL; the
			// operator needs to paste a real endpoint into parish.toml.
			warnMissingTileUrl(tileSource.id);
		}
		layers.push({
			id: 'background',
			type: 'background',
			paint: { 'background-color': theme.panelBg }
		});
	}

	// 2. Edges (graph connections with footprint-weighted width).
	//
	// Split into two layers — solid for normal edges, dashed for frontier —
	// because MapLibre GL JS does not support data-driven expressions for
	// `line-dasharray`. A single layer with `['case', ['get', 'frontier'], ...]`
	// on `line-dasharray` causes silent style validation failure (the `load`
	// event never fires, leaving the canvas blank).
	// 2a. Solid edges (visited/known connections).
	layers.push({
		id: 'edges-solid',
		type: 'line',
		source: 'edges',
		filter: ['!', ['get', 'frontier']],
		layout: { 'line-cap': 'round', 'line-join': 'round' },
		paint: {
			'line-color': ['case', ['get', 'traversing'], theme.accent, theme.border],
			'line-opacity': ['case', ['get', 'traversing'], 1, 0.85],
			'line-width': [
				'interpolate',
				['linear'],
				['zoom'],
				10,
				[
					'case',
					['get', 'traversing'],
					4,
					['+', 1, ['*', ['get', 'traversalWeight'], 2]]
				],
				18,
				[
					'case',
					['get', 'traversing'],
					7,
					['+', 2, ['*', ['get', 'traversalWeight'], 4]]
				]
			]
		}
	});

	// 2b. Dashed frontier edges (fog-of-war).
	layers.push({
		id: 'edges-frontier',
		type: 'line',
		source: 'edges',
		filter: ['get', 'frontier'],
		layout: { 'line-cap': 'round', 'line-join': 'round' },
		paint: {
			'line-color': theme.muted,
			'line-opacity': 0.4,
			'line-width': [
				'interpolate', ['linear'], ['zoom'],
				10, ['+', 1, ['*', ['get', 'traversalWeight'], 2]],
				18, ['+', 2, ['*', ['get', 'traversalWeight'], 4]]
			],
			'line-dasharray': [2, 1.5]
		}
	});

	// 3. Glow underlay for lit and player locations.
	layers.push({
		id: 'location-glow',
		type: 'circle',
		source: 'locations',
		paint: {
			'circle-radius': [
				'case',
				['get', 'isPlayer'],
				variant === 'minimap' ? 12 : 16,
				['get', 'lit'],
				variant === 'minimap' ? 8 : 10,
				0.01
			],
			'circle-blur': [
				'case',
				['get', 'isPlayer'],
				0.9,
				['get', 'lit'],
				0.75,
				0
			],
			'circle-color': theme.accent,
			'circle-stroke-color': [
				'case',
				['get', 'isPlayer'],
				theme.accent,
				['get', 'lit'],
				theme.accent,
				theme.panelBg
			],
			'circle-stroke-width': [
				'case',
				['get', 'isPlayer'],
				2.5,
				['get', 'lit'],
				1.5,
				0
			],
			'circle-opacity': [
				'case',
				['any', ['get', 'isPlayer'], ['get', 'lit']],
				1,
				0
			],
			'circle-stroke-opacity': [
				'case',
				['get', 'visited'],
				1,
				0.5
			]
		}
	});

	// 4. Location icons (custom Phosphor glyphs registered at runtime).
	layers.push({
		id: 'location-circles',
		type: 'symbol',
		source: 'locations',
		layout: {
			'icon-image': ['concat', 'icon-', ['get', 'icon']],
			// Icon sprites are drawn onto a 64px canvas (see drawIconImage in
			// controller.ts), so rendered pixel size is `64 * icon-size`. The
			// minimap runs a few points larger than the full map so locations
			// remain readable at the panel's 240px viewport. Icons scale
			// linearly with zoom and max out at ~3× their zoomed-out size so
			// a fully zoomed-in view reads comfortably without dominating
			// the map tiles.
			'icon-size': [
				'interpolate',
				['linear'],
				['zoom'],
				10,
				variant === 'minimap'
					? ['case', ['get', 'isPlayer'], 0.32, 0.26]
					: ['case', ['get', 'isPlayer'], 0.3, 0.22],
				18,
				variant === 'minimap'
					? ['case', ['get', 'isPlayer'], 0.96, 0.78]
					: ['case', ['get', 'isPlayer'], 0.9, 0.66]
			],
			'icon-allow-overlap': true,
			'icon-ignore-placement': true
		},
		paint: {
			'icon-color': [
				'case',
				['get', 'isPlayer'],
				theme.fg,
				['get', 'lit'],
				theme.accent,
				['get', 'adjacent'],
				theme.accent,
				theme.muted
			],
			'icon-opacity': [
				'case',
				['get', 'visited'],
				1,
				0.55
			],
			// Halo matches the label halo so icons read against the
			// desaturated historic map tiles — the pre-fix 0.8 px width was
			// invisible once the icon color sat near the parchment tones.
			// Unvisited locations get a thinner halo so they read as softer
			// fog-of-war hints rather than equal-weight siblings.
			'icon-halo-color': theme.bg,
			'icon-halo-width': ['case', ['get', 'visited'], 1.5, 0.6],
			'icon-halo-blur': 0.2
		}
	});

	// 5. Location labels — the whole point of this migration.
	//
	//    MapLibre's symbol layer does the collision-aware placement we were
	//    hand-rolling in `map-labels.ts`: variable anchors pick the best side
	//    per label, `symbol-sort-key` makes important labels win any overlap,
	//    and zoom-aware placement fades labels in/out as they declutter.
	layers.push({
		id: 'location-labels',
		type: 'symbol',
		source: 'locations',
		layout: {
			'text-field': ['get', 'name'],
			'text-font': ['Open Sans Regular'],
			'text-size': [
				'interpolate',
				['linear'],
				['zoom'],
				10,
				10,
				14,
				12,
				18,
				14
			],
			'text-variable-anchor': [
				'top',
				'bottom',
				'left',
				'right',
				'top-left',
				'top-right',
				'bottom-left',
				'bottom-right'
			],
			'text-radial-offset': 1.2,
			'text-justify': 'auto',
			'text-padding': 3,
			'text-max-width': 8,
			'text-allow-overlap': false,
			'text-ignore-placement': false,
			// Lower sort key = placed first = wins collisions. Player and
			// adjacent locations always get labeled; unvisited frontier
			// nodes give way to anyone interesting.
			'symbol-sort-key': [
				'case',
				['get', 'isPlayer'],
				0,
				['get', 'adjacent'],
				1,
				['get', 'visited'],
				2,
				3
			]
		},
		paint: {
			'text-color': [
				'case',
				['get', 'isPlayer'],
				theme.fg,
				['get', 'lit'],
				theme.accent,
				theme.muted
			],
			'text-halo-color': theme.bg,
			'text-halo-width': 1.4,
			'text-halo-blur': 0.2,
			'text-opacity': [
				'case',
				['get', 'visited'],
				1,
				0.55
			]
		}
	});

	const sources: StyleSpecification['sources'] = {
		locations: {
			type: 'geojson',
			data: { type: 'FeatureCollection', features: [] }
		},
		edges: {
			type: 'geojson',
			data: { type: 'FeatureCollection', features: [] }
		}
	};
	if (hasUsableTiles) {
		const rasterSource: RasterSourceSpecification = {
			type: 'raster',
			tiles: [tileSource!.url],
			tileSize: tileSource!.tile_size,
			minzoom: tileSource!.minzoom,
			maxzoom: tileSource!.maxzoom,
			attribution: tileSource!.attribution
		};
		if (tileSource!.tms) rasterSource.scheme = 'tms';
		sources[rasterSourceId] = rasterSource;
	}

	return {
		version: 8,
		glyphs: glyphsUrl,
		sources,
		layers
	};
}

// Shown once per missing id so toggling between sources doesn't spam the
// console. Scoped module-level so the set persists across `buildStyle` calls.
const warnedMissingIds = new Set<string>();
function warnMissingTileUrl(id: string) {
	if (warnedMissingIds.has(id)) return;
	warnedMissingIds.add(id);
	// eslint-disable-next-line no-console
	console.warn(
		`[tiles] source '${id}' has no URL; falling back to flat background. ` +
			`Set [engine.map.tile_sources.${id}] url = "..." in parish.toml ` +
			'or see docs/design/map-evolution.md.'
	);
}
