/**
 * Thin Svelte-agnostic wrapper around a `maplibregl.Map` instance.
 *
 * Both `MapPanel.svelte` (minimap) and `FullMapOverlay.svelte` (full map)
 * instantiate a `MapController` in `onMount` and drive it from `$effect`
 * blocks whenever their reactive state changes. Keeping the imperative
 * MapLibre API in one place lets the Svelte components stay thin and lets
 * us unit-test the controller directly.
 *
 * Features handled here:
 *   - map lifecycle (init, destroy)
 *   - pushing location / edge GeoJSON into the style's sources
 *   - centering & fitting the camera to game state
 *   - animated travel dot via an HTMLMarker
 *   - click + hover event wiring on the locations layer
 *   - projecting lat/lon → screen coordinates for HTML overlays
 *     (used by the minimap's off-screen radar indicators)
 */

import maplibregl, {
	Map as MapLibreMap,
	Marker,
	LngLatBounds,
	type LngLatLike,
	type MapMouseEvent,
	type MapGeoJSONFeature
} from 'maplibre-gl';
import type { MapData, TileSource, TravelWaypoint } from '$lib/types';
import { buildStyle, readThemeColors, type MapVariant } from './style';
import {
	locationsToGeoJSON,
	edgesToGeoJSON,
	computeOffMapCounts,
	type LocationFeatureProps,
	type EdgeFeatureProps
} from './geojson';
import type { FeatureCollection, Point, LineString } from 'geojson';

/** Options passed to the controller at construction time. */
export interface MapControllerOptions {
	container: HTMLElement;
	variant: MapVariant;
	/** When true, disables pan/zoom/rotate interaction (minimap). */
	interactive: boolean;
	/** Initial zoom level. Minimap uses a fixed zoom; full map fits bounds. */
	initialZoom?: number;
	/** Raster tile source to use for the base layer. Undefined or a source
	 *  with an empty URL yields a flat-background fallback. */
	tileSource?: TileSource;
}

/** Callback payload emitted on location click. */
export interface LocationClickInfo {
	id: string;
	name: string;
	adjacent: boolean;
}

/** Callback payload emitted on location hover enter. */
export interface LocationHoverInfo {
	id: string;
	name: string;
	visited: boolean;
	indoor: boolean;
	travelMinutes: number;
}

/** Travel animation handle returned by `animateTravel`. */
interface TravelAnimation {
	marker: Marker;
	rafId: number;
}

export class MapController {
	private map: MapLibreMap;
	private variant: MapVariant;
	private ready = false;
	private pendingMapData: MapData | null = null;
	/** Last `MapData` we rendered. Retained so that `setTileSource()` can
	 *  re-push the overlay GeoJSON after MapLibre's `setStyle()` wipes all
	 *  sources and layers. */
	private lastMapData: MapData | null = null;
	/** Ids of currently-visible locations (filtered set, or all if no filter). */
	private lastVisibleIds: Set<string> | null = null;
	/** Active travel animation, cleared when travel ends. */
	private travelAnim: TravelAnimation | null = null;
	/** Registered click handler, if any. */
	private clickHandler: ((info: LocationClickInfo) => void) | null = null;
	/** Registered hover handler, if any. */
	private hoverEnterHandler: ((info: LocationHoverInfo) => void) | null = null;
	private hoverLeaveHandler: (() => void) | null = null;
	/** Current tile source, mirrored so `setTileSource` can rebuild the style. */
	private tileSource: TileSource | undefined;

	constructor(options: MapControllerOptions) {
		this.variant = options.variant;
		this.tileSource = options.tileSource;
		const theme = readThemeColors();
		this.map = new maplibregl.Map({
			container: options.container,
			style: buildStyle(options.variant, theme, this.tileSource),
			center: [-8.15, 53.59], // Kiltoom/Kilteevan reference center
			zoom: options.initialZoom ?? (options.variant === 'minimap' ? 14 : 13),
			interactive: options.interactive,
			attributionControl: options.variant === 'full' ? undefined : false,
			dragRotate: false,
			pitchWithRotate: false,
			touchZoomRotate: options.interactive,
			keyboard: false
		});

		this.map.on('load', () => {
			this.ready = true;
			this.wireLayerEvents();
			if (this.pendingMapData) {
				this.updateMap(this.pendingMapData);
				this.pendingMapData = null;
			}
		});
	}

	/**
	 * Swaps the base tile source at runtime.
	 *
	 * MapLibre's `setStyle()` wipes all existing sources and layers, so after
	 * the new style's `styledata` event fires we re-push the last `MapData`
	 * to restore the location/edge overlays. Using `.once()` avoids re-entry
	 * on repeated `styledata` fires.
	 *
	 * Called by map components' `$effect`s when the `tiles` store's active id
	 * changes (driven by the `tiles-switch` event from `/tiles`).
	 */
	setTileSource(source: TileSource | undefined): void {
		this.tileSource = source;
		const theme = readThemeColors();
		this.ready = false;
		this.map.setStyle(buildStyle(this.variant, theme, source));
		this.map.once('styledata', () => {
			this.ready = true;
			this.wireLayerEvents();
			const data = this.pendingMapData ?? this.lastMapData;
			this.pendingMapData = null;
			if (data) this.updateMap(data, this.lastVisibleIds ?? undefined);
		});
	}

	/**
	 * Updates the map's location and edge sources from the latest `MapData`.
	 *
	 * On the minimap, pass `visibleIds` to restrict which locations and edges
	 * are rendered (typically the set of nodes within MINIMAP_HOP_RADIUS of
	 * the player). On the full map, pass undefined to render everything.
	 */
	updateMap(mapData: MapData, visibleIds?: ReadonlySet<string>): void {
		// Retained for post-`setStyle` re-push in `setTileSource`.
		this.lastMapData = mapData;

		if (!this.ready) {
			this.pendingMapData = mapData;
			return;
		}

		this.lastVisibleIds = visibleIds ? new Set(visibleIds) : null;

		// Compute off-map edge counts for the minimap's continuation stubs.
		const offMapCounts = visibleIds
			? computeOffMapCounts(mapData.edges, visibleIds)
			: undefined;

		const locationFC = locationsToGeoJSON(mapData, {
			filterIds: visibleIds,
			offMapCounts
		});
		const edgeFC = edgesToGeoJSON(mapData, { filterIds: visibleIds });

		setSourceData(this.map, 'locations', locationFC);
		setSourceData(this.map, 'edges', edgeFC);
	}

	/**
	 * Centers the camera on the given lat/lon. Used by the minimap whenever
	 * the player's location changes, to keep them in the middle of the view.
	 */
	setCenter(lat: number, lon: number, animate = true): void {
		const center: LngLatLike = [lon, lat];
		if (animate) {
			this.map.easeTo({ center, duration: 400 });
		} else {
			this.map.jumpTo({ center });
		}
	}

	/**
	 * Fits the map bounds to the given lat/lon box with padding.
	 * Used by the full map on mount to frame the whole parish at once.
	 */
	fitBounds(
		corners: Array<{ lat: number; lon: number }>,
		padding = 60
	): void {
		if (corners.length === 0) return;
		const bounds = new LngLatBounds();
		for (const c of corners) {
			bounds.extend([c.lon, c.lat]);
		}
		this.map.fitBounds(bounds, { padding, duration: 0, maxZoom: 16 });
	}

	/**
	 * Starts (or updates) a travel-dot animation along the given waypoints.
	 * Creates an HTMLMarker with a pulsing dot element; on each frame the
	 * marker's lat/lon is interpolated between consecutive waypoints.
	 *
	 * Call `stopTravel()` when the animation should end.
	 */
	startTravel(waypoints: TravelWaypoint[], durationMs: number): void {
		this.stopTravel();
		if (waypoints.length < 2) return;

		const el = document.createElement('div');
		el.className = 'travel-dot-marker';

		const marker = new maplibregl.Marker({ element: el, anchor: 'center' })
			.setLngLat([waypoints[0].lon, waypoints[0].lat])
			.addTo(this.map);

		const startTs = performance.now();
		const segCount = waypoints.length - 1;

		const tick = () => {
			const now = performance.now();
			const t = Math.min(1, Math.max(0, (now - startTs) / durationMs));
			const segFloat = t * segCount;
			const segIdx = Math.min(Math.floor(segFloat), segCount - 1);
			const segT = segFloat - segIdx;
			const from = waypoints[segIdx];
			const to = waypoints[segIdx + 1];
			const lat = from.lat + (to.lat - from.lat) * segT;
			const lon = from.lon + (to.lon - from.lon) * segT;
			marker.setLngLat([lon, lat]);
			if (t < 1 && this.travelAnim) {
				this.travelAnim.rafId = requestAnimationFrame(tick);
			}
		};

		this.travelAnim = { marker, rafId: requestAnimationFrame(tick) };
	}

	/** Stops and removes any active travel animation. */
	stopTravel(): void {
		if (!this.travelAnim) return;
		cancelAnimationFrame(this.travelAnim.rafId);
		this.travelAnim.marker.remove();
		this.travelAnim = null;
	}

	/**
	 * Projects a lat/lon into pixel coordinates relative to the map container.
	 * Used by the minimap's off-screen radar indicator overlay.
	 */
	projectToScreen(lat: number, lon: number): { x: number; y: number } {
		const p = this.map.project([lon, lat]);
		return { x: p.x, y: p.y };
	}

	/** Returns the size of the map container in pixels. */
	getContainerSize(): { width: number; height: number } {
		const canvas = this.map.getCanvas();
		return { width: canvas.clientWidth, height: canvas.clientHeight };
	}

	/** Registers a handler called when a location is clicked. */
	onLocationClick(handler: (info: LocationClickInfo) => void): void {
		this.clickHandler = handler;
	}

	/** Registers handlers called on location hover enter / leave. */
	onLocationHover(
		enter: (info: LocationHoverInfo) => void,
		leave: () => void
	): void {
		this.hoverEnterHandler = enter;
		this.hoverLeaveHandler = leave;
	}

	/** Cleans up the underlying MapLibre instance and any running animation. */
	destroy(): void {
		this.stopTravel();
		this.map.remove();
	}

	/**
	 * Wires click & hover listeners onto the `location-circles` layer once
	 * the map has finished loading. A single layer handles both the visual
	 * and the hit-testing — clicks on the invisible-at-zoom labels still
	 * fall through to this because the circle sits beneath.
	 */
	private wireLayerEvents(): void {
		const canvas = this.map.getCanvas();

		const handleClick = (e: MapMouseEvent & { features?: MapGeoJSONFeature[] }) => {
			const feat = e.features?.[0];
			if (!feat || !this.clickHandler) return;
			const props = feat.properties as LocationFeatureProps;
			this.clickHandler({
				id: props.id,
				name: props.name,
				adjacent: !!props.adjacent
			});
		};

		const handleMouseEnter = (
			e: MapMouseEvent & { features?: MapGeoJSONFeature[] }
		) => {
			const feat = e.features?.[0];
			if (!feat) return;
			const props = feat.properties as LocationFeatureProps;
			if (props.adjacent) canvas.style.cursor = 'pointer';
			this.hoverEnterHandler?.({
				id: props.id,
				name: props.name,
				visited: !!props.visited,
				indoor: !!props.indoor,
				travelMinutes: props.travelMinutes ?? 0
			});
		};

		const handleMouseLeave = () => {
			canvas.style.cursor = '';
			this.hoverLeaveHandler?.();
		};

		this.map.on('click', 'location-circles', handleClick);
		this.map.on('click', 'location-labels', handleClick);
		this.map.on('mouseenter', 'location-circles', handleMouseEnter);
		this.map.on('mouseleave', 'location-circles', handleMouseLeave);
	}
}

/**
 * Type-safe wrapper around `map.getSource(id).setData(...)` that silently
 * no-ops if the source hasn't been created yet (e.g. during style swap).
 */
function setSourceData(
	map: MapLibreMap,
	id: string,
	data:
		| FeatureCollection<Point, LocationFeatureProps>
		| FeatureCollection<LineString, EdgeFeatureProps>
): void {
	const source = map.getSource(id);
	if (source && source.type === 'geojson') {
		(source as maplibregl.GeoJSONSource).setData(data);
	}
}
