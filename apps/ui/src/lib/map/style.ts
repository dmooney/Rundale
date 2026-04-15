/**
 * MapLibre style JSON factory for the Parish map views.
 *
 * Produces a `StyleSpecification` tailored to either the minimap or the
 * full-parish overlay. The style wires up:
 *
 *   - a raster OSM base (full map only) or a flat panel-bg (minimap)
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
		muted: get('--color-muted', '#76663b')
	};
}

/** MapLibre demo glyphs endpoint — free, no auth, network-dependent. */
// TODO: bundle Open Sans glyph PBFs as static assets to work fully offline.
const GLYPHS_URL = 'https://demotiles.maplibre.org/font/{fontstack}/{range}.pbf';

/**
 * Builds a MapLibre style spec for the given map variant and theme.
 *
 * The style has two GeoJSON sources (`locations` and `edges`) that start
 * empty — the controller populates them via `setData()` as game state
 * changes. On the full map a raster base is added beneath, sourced from
 * the `tileSource` parameter (ships with OSM by default; the `/tiles`
 * slash command swaps this via `MapController.setTileSource()`).
 *
 * Passing a `tileSource` with an empty `url` (e.g. a user-added source
 * that hasn't had its URL filled in yet) falls back to the flat-bg layer
 * with a one-shot console warning — the feature flag can stay on without
 * a live endpoint.
 */
export function buildStyle(
	variant: MapVariant,
	theme: ThemeColors,
	tileSource?: TileSource
): StyleSpecification {
	const layers: LayerSpecification[] = [];
	const rasterSourceId = 'map-tiles';
	const hasUsableTiles = variant === 'full' && tileSource && tileSource.url.length > 0;

	// 1. Base layer — flat color on the minimap, configured raster on the full map.
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
		if (variant === 'full' && tileSource && tileSource.url.length === 0) {
			// Informational — a source was registered without a URL; the
			// operator needs to paste a real endpoint into parish.toml.
			warnMissingTileUrl(tileSource.id);
		}
		// Minimap: flat panel background, no tiles.
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
			'line-color': theme.border,
			'line-opacity': 0.85,
			'line-width': [
				'interpolate', ['linear'], ['zoom'],
				10, ['+', 1, ['*', ['get', 'traversalWeight'], 2]],
				18, ['+', 2, ['*', ['get', 'traversalWeight'], 4]]
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

	// 3. Location dots — small circle beneath each symbol so there's always
	//    something visible even when the icon sprite fails to register.
	layers.push({
		id: 'location-circles',
		type: 'circle',
		source: 'locations',
		paint: {
			'circle-radius': [
				'case',
				['get', 'isPlayer'],
				variant === 'minimap' ? 9 : 11,
				variant === 'minimap' ? 5 : 7
			],
			'circle-color': [
				'case',
				['get', 'isPlayer'],
				theme.accent,
				['get', 'lit'],
				theme.accent,
				theme.panelBg
			],
			'circle-stroke-color': [
				'case',
				['get', 'isPlayer'],
				theme.fg,
				['get', 'adjacent'],
				theme.accent,
				theme.muted
			],
			'circle-stroke-width': [
				'case',
				['get', 'isPlayer'],
				2,
				1.25
			],
			'circle-opacity': [
				'case',
				['get', 'visited'],
				1,
				0.45
			],
			'circle-stroke-opacity': [
				'case',
				['get', 'visited'],
				1,
				0.5
			]
		}
	});

	// 4. Location labels — the whole point of this migration.
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
		glyphs: GLYPHS_URL,
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
