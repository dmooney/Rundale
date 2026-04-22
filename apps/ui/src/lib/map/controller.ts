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
	edgeKey,
	type LocationFeatureProps,
	type EdgeFeatureProps
} from './geojson';
import type { FeatureCollection, Point, LineString } from 'geojson';
import { ICON_PATHS, type LocationIcon } from '$lib/map-icons';

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
	/** Canonical edge keys for the active travel path (for line highlighting). */
	private activeTravelEdgeKeys = new Set<string>();
	/** Registered click handler, if any. */
	private clickHandler: ((info: LocationClickInfo) => void) | null = null;
	/** Registered hover handler, if any. */
	private hoverEnterHandler: ((info: LocationHoverInfo) => void) | null = null;
	private hoverLeaveHandler: (() => void) | null = null;
	/** Current tile source, mirrored so `setTileSource` can rebuild the style. */
	private tileSource: TileSource | undefined;
	/** Whether layer-delegated event handlers are currently attached. MapLibre
	 *  stores per-layer delegated listeners on the map instance itself, so
	 *  they survive `setStyle()` — if we call `wireLayerEvents` twice without
	 *  tearing the previous set down first, every click fires N handlers and
	 *  the same `submitInput` runs N times. */
	private layerEventsWired = false;
	private layerClickHandler:
		| ((e: MapMouseEvent & { features?: MapGeoJSONFeature[] }) => void)
		| null = null;
	private layerMouseEnterHandler:
		| ((e: MapMouseEvent & { features?: MapGeoJSONFeature[] }) => void)
		| null = null;
	private layerMouseLeaveHandler: (() => void) | null = null;

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
			registerLocationIcons(this.map);
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
			registerLocationIcons(this.map);
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
		const edgeFC = edgesToGeoJSON(mapData, {
			filterIds: visibleIds,
			traversingEdgeKeys: this.activeTravelEdgeKeys
		});

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
	 *
	 * The dot is interpolated along the waypoint polyline in world coordinates
	 * (distance-proportional over `durationMs`), so it tracks the true path
	 * regardless of where the user has panned the camera. The camera animates
	 * independently: when `targetBounds` is supplied (minimap case) we
	 * `fitBounds` so center AND zoom interpolate smoothly to the destination
	 * neighborhood; otherwise (full map) we leave the camera alone so user
	 * pans aren't overridden mid-travel.
	 *
	 * Call `stopTravel()` when the animation should end.
	 */
	startTravel(
		waypoints: TravelWaypoint[],
		durationMs: number,
		targetBounds?: Array<{ lat: number; lon: number }>
	): void {
		this.stopTravel();
		if (waypoints.length < 2) return;
		this.activeTravelEdgeKeys = buildTravelEdgeKeys(waypoints);
		if (this.lastMapData) {
			this.updateMap(this.lastMapData, this.lastVisibleIds ?? undefined);
		}

		const el = document.createElement('div');
		el.className = 'travel-dot-marker';

		const marker = new maplibregl.Marker({ element: el, anchor: 'center' })
			.setLngLat([waypoints[0].lon, waypoints[0].lat])
			.addTo(this.map);

		if (targetBounds && targetBounds.length > 0) {
			const bounds = new LngLatBounds();
			for (const c of targetBounds) bounds.extend([c.lon, c.lat]);
			this.map.fitBounds(bounds, {
				padding: 16,
				maxZoom: 16,
				duration: durationMs,
				easing: (t) => t,
				linear: true
			});
		}

		// Flat-earth distance is fine at parish scale — all travel fits inside
		// a few kilometres, so lat/lon Euclidean approximates arc length well
		// enough for a pulsing dot.
		const segLengths: number[] = [];
		let totalLength = 0;
		for (let i = 1; i < waypoints.length; i += 1) {
			const a = waypoints[i - 1];
			const b = waypoints[i];
			const d = Math.hypot(b.lon - a.lon, b.lat - a.lat);
			segLengths.push(d);
			totalLength += d;
		}

		const startTime = performance.now();
		const tick = () => {
			const t = durationMs > 0
				? Math.min(1, (performance.now() - startTime) / durationMs)
				: 1;
			const [lon, lat] = positionAlongPath(waypoints, segLengths, totalLength, t);
			marker.setLngLat([lon, lat]);
			if (t < 1 && this.travelAnim) {
				this.travelAnim.rafId = requestAnimationFrame(tick);
			}
		};

		this.travelAnim = { marker, rafId: requestAnimationFrame(tick) };
	}

	/** Stops and removes any active travel animation. */
	stopTravel(): void {
		const hadActivePath = this.activeTravelEdgeKeys.size > 0;
		this.activeTravelEdgeKeys.clear();
		if (hadActivePath && this.lastMapData) {
			this.updateMap(this.lastMapData, this.lastVisibleIds ?? undefined);
		}
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
		this.unwireLayerEvents();
		this.map.remove();
	}

	/**
	 * Wires click & hover listeners onto the location layers.
	 *
	 * Called on first `load` and again after every `setStyle` (which wipes
	 * layers but NOT MapLibre's `_delegatedListeners` map). We stash the
	 * bound handler refs and tear them down before re-adding — otherwise
	 * each `setTileSource` call stacks another handler and every click
	 * fires `submitInput` N times.
	 */
	private wireLayerEvents(): void {
		this.unwireLayerEvents();

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

		this.layerClickHandler = handleClick;
		this.layerMouseEnterHandler = handleMouseEnter;
		this.layerMouseLeaveHandler = handleMouseLeave;

		this.map.on('click', 'location-circles', handleClick);
		this.map.on('click', 'location-labels', handleClick);
		this.map.on('mouseenter', 'location-circles', handleMouseEnter);
		this.map.on('mouseleave', 'location-circles', handleMouseLeave);
		this.layerEventsWired = true;
	}

	private unwireLayerEvents(): void {
		if (!this.layerEventsWired) return;
		if (this.layerClickHandler) {
			this.map.off('click', 'location-circles', this.layerClickHandler);
			this.map.off('click', 'location-labels', this.layerClickHandler);
		}
		if (this.layerMouseEnterHandler) {
			this.map.off('mouseenter', 'location-circles', this.layerMouseEnterHandler);
		}
		if (this.layerMouseLeaveHandler) {
			this.map.off('mouseleave', 'location-circles', this.layerMouseLeaveHandler);
		}
		this.layerClickHandler = null;
		this.layerMouseEnterHandler = null;
		this.layerMouseLeaveHandler = null;
		this.layerEventsWired = false;
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

/**
 * Returns `[lon, lat]` at fractional progress `t ∈ [0, 1]` along the polyline
 * formed by `waypoints`, weighted by segment length so progress matches
 * distance travelled rather than waypoint count.
 */
export function positionAlongPath(
	waypoints: TravelWaypoint[],
	segLengths: number[],
	totalLength: number,
	t: number
): [number, number] {
	if (waypoints.length === 0) return [0, 0];
	if (t <= 0 || totalLength === 0) {
		const first = waypoints[0];
		return [first.lon, first.lat];
	}
	if (t >= 1) {
		const last = waypoints[waypoints.length - 1];
		return [last.lon, last.lat];
	}
	const target = t * totalLength;
	let consumed = 0;
	for (let i = 0; i < segLengths.length; i += 1) {
		const segLen = segLengths[i];
		if (consumed + segLen >= target) {
			const frac = segLen === 0 ? 0 : (target - consumed) / segLen;
			const a = waypoints[i];
			const b = waypoints[i + 1];
			return [a.lon + (b.lon - a.lon) * frac, a.lat + (b.lat - a.lat) * frac];
		}
		consumed += segLen;
	}
	const last = waypoints[waypoints.length - 1];
	return [last.lon, last.lat];
}

function buildTravelEdgeKeys(waypoints: TravelWaypoint[]): Set<string> {
	const keys = new Set<string>();
	for (let i = 0; i < waypoints.length - 1; i += 1) {
		const a = waypoints[i];
		const b = waypoints[i + 1];
		keys.add(edgeKey(a.id, b.id));
	}
	return keys;
}

function registerLocationIcons(map: MapLibreMap): void {
	for (const [icon, path] of Object.entries(ICON_PATHS) as Array<[LocationIcon, string]>) {
		const imageId = `icon-${icon}`;
		if (map.hasImage(imageId)) continue;
		const image = drawIconImage(path);
		if (!image) continue;
		map.addImage(imageId, image, { sdf: true });
	}
}

// Returns null when the host lacks a usable 2D canvas (jsdom in unit tests).
// Real browsers always provide one; MapLibre then falls back to a square
// placeholder for `icon-image: icon-…` which keeps the rest of the style
// valid rather than hard-failing the test render.
function drawIconImage(pathData: string): ImageData | null {
	const size = 64;
	const canvas = document.createElement('canvas');
	canvas.width = size;
	canvas.height = size;
	const ctx = canvas.getContext('2d');
	if (!ctx || typeof Path2D === 'undefined') return null;
	ctx.clearRect(0, 0, size, size);
	ctx.fillStyle = '#ffffff';
	ctx.scale(size / 256, size / 256);
	const path = new Path2D(pathData);
	ctx.fill(path);
	return ctx.getImageData(0, 0, size, size);
}
