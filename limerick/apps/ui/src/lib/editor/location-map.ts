/**
 * MapLibre instance lifecycle + drag-handler controller for the editor's
 * location map (#1200 TD-043).
 *
 * Extracted from `LocationDetail.svelte`. The component owns reactive Svelte
 * state (selection, locations, dirty flags); this controller owns the
 * imperative MapLibre `Map` instance, its sources/layers, the marker-drag
 * state machine, and the camera-animation memo. The component drives it
 * through the callback hooks in [`EditorLocationMapHooks`] so no reactive
 * `$state` leaks into the controller and the pure map-data math stays in
 * `$lib/editor-map`.
 */

import {
	Map as MapLibreMap,
	NavigationControl,
	type GeoJSONSource,
} from 'maplibre-gl';

import {
	applyDraggedCoordinates,
	buildEditorMapData,
	getEditorMapCenter,
} from '$lib/editor-map';
import { getUiConfig } from '$lib/ipc';
import { buildStyle, readThemeColors } from '$lib/map/style';
import { configureMapLibreWorker } from '$lib/maplibre-worker';
import type { LocationData } from '$lib/editor-types';
import type { TileSource } from '$lib/types';

/** Callbacks the controller uses to read component state and request actions. */
export interface EditorLocationMapHooks {
	/** Current full location list (reactive snapshot, read on demand). */
	getLocations: () => LocationData[];
	/** Currently selected location id, or `null`. */
	getSelectedId: () => number | null;
	/** Currently selected location record, or `null`. */
	getSelectedLocation: () => LocationData | null;
	/** Whether the component has been torn down (guards async map init). */
	isDisposed: () => boolean;
	/** Selects a location by id (sets the editor store). */
	onSelect: (id: number) => void;
	/** Shift-click on a non-selected node toggles an edge to it. */
	toggleConnection: (targetId: number) => Promise<void>;
	/** Commits a dragged location's new coordinates. */
	updateLocationById: (
		id: number,
		mutator: (location: LocationData) => LocationData,
	) => Promise<void>;
}

function readLocationId(event: {
	features?: Array<{ properties?: { id?: number | string } }>;
}): number | null {
	const rawId = event.features?.[0]?.properties?.id;
	const id = typeof rawId === 'number' ? rawId : Number(rawId);
	return Number.isNaN(id) ? null : id;
}

export class EditorLocationMap {
	private map: MapLibreMap | null = null;
	private mapLoaded = false;
	private mapInitializing = false;

	// Marker-drag state.
	private dragTargetId: number | null = null;
	private dragMoved = false;
	private dragMouseupHandler: (() => void) | null = null;

	// Track the last selection + coords we animated to, so updates that don't
	// actually move the camera skip the easeTo animation (#410). Without this,
	// every keystroke in a text field would dispatch a full camera pan —
	// persistLocations swaps a new `locations` array into the store, re-firing
	// the reactive effect even when the map center hasn't moved. We key on
	// (selectedId, lat, lon) so coordinate-changing actions (drag release,
	// nudge, lat/lon field edits) *do* recenter — per codex P1 on #559.
	private lastAnimatedSelectedId: number | null = null;
	private lastAnimatedLat: number | null = null;
	private lastAnimatedLon: number | null = null;

	constructor(private readonly hooks: EditorLocationMapHooks) {}

	/** Whether a `maplibregl.Map` currently exists. */
	get exists(): boolean {
		return this.map !== null;
	}

	setMapData(
		nextLocations: LocationData[],
		nextSelectedId: number | null,
		preview?: { id: number; lat: number; lon: number },
	): void {
		const map = this.map;
		if (!map || !this.mapLoaded) return;
		const { features, edgeFeatures } = buildEditorMapData(
			nextLocations,
			nextSelectedId,
			preview,
		);
		(map.getSource('editor-locations') as GeoJSONSource)?.setData({
			type: 'FeatureCollection',
			features,
		});
		(map.getSource('editor-edges') as GeoJSONSource)?.setData({
			type: 'FeatureCollection',
			features: edgeFeatures,
		});
		const center = getEditorMapCenter(features, nextSelectedId, preview);
		if (!center) return;
		const [lon, lat] = center;
		// Animate only when something that affects the camera position actually
		// changes: the selection, or the selected marker's coordinates.
		// Unrelated field edits that bumped the locations array must not cause
		// camera jitter. Preview frames (drag in progress) always animate so the
		// camera tracks the cursor.
		const selectionChanged = nextSelectedId !== this.lastAnimatedSelectedId;
		const coordsChanged =
			lat !== this.lastAnimatedLat || lon !== this.lastAnimatedLon;
		if (!preview && !selectionChanged && !coordsChanged) return;
		map.easeTo({ center: [lon, lat], duration: 250 });
		if (!preview) {
			// Preview frames stream continuously during a drag; don't mark the
			// selection as "animated" from them or we'd suppress the final
			// settle-on-release animation.
			this.lastAnimatedSelectedId = nextSelectedId;
			this.lastAnimatedLat = lat;
			this.lastAnimatedLon = lon;
		}
	}

	destroy(): void {
		this.mapLoaded = false;
		if (this.dragMouseupHandler) {
			window.removeEventListener('mouseup', this.dragMouseupHandler);
			this.dragMouseupHandler = null;
		}
		this.map?.remove();
		this.map = null;
		// Reset animation memo so a later remount (deselect → reselect the same
		// item, or component re-mount) still animates the first setMapData call
		// — codex P2 on #559.
		this.lastAnimatedSelectedId = null;
		this.lastAnimatedLat = null;
		this.lastAnimatedLon = null;
	}

	async ensure(mapContainer: HTMLDivElement): Promise<void> {
		if (this.map || this.mapInitializing || this.hooks.isDisposed()) return;
		this.mapInitializing = true;

		let initialTile: TileSource | undefined;
		try {
			const cfg = await getUiConfig();
			initialTile =
				cfg.tile_sources.find((t) => t.id === cfg.active_tile_source) ??
				cfg.tile_sources[0];
		} catch {
			initialTile = undefined;
		}

		if (this.map || this.hooks.isDisposed()) {
			this.mapInitializing = false;
			return;
		}

		configureMapLibreWorker();
		const nextMap = new MapLibreMap({
			container: mapContainer,
			style: buildStyle('full', readThemeColors(), initialTile),
			center: [-8.0, 53.5],
			zoom: 12,
			boxZoom: false,
		});
		this.map = nextMap;
		nextMap.addControl(
			new NavigationControl({ showCompass: false }),
			'top-right',
		);
		nextMap.on('load', () => {
			if (this.map !== nextMap || this.hooks.isDisposed()) return;
			this.mapLoaded = true;
			const canvas = nextMap.getCanvas();
			nextMap.addSource('editor-locations', {
				type: 'geojson',
				data: { type: 'FeatureCollection', features: [] },
			});
			nextMap.addSource('editor-edges', {
				type: 'geojson',
				data: { type: 'FeatureCollection', features: [] },
			});
			const { mapEdge, mapSelected, mapRelative, mapStroke } =
				readThemeColors();
			nextMap.addLayer({
				id: 'editor-edges',
				type: 'line',
				source: 'editor-edges',
				paint: { 'line-color': mapEdge, 'line-width': 2, 'line-opacity': 0.85 },
			});
			nextMap.addLayer({
				id: 'editor-locations',
				type: 'circle',
				source: 'editor-locations',
				paint: {
					'circle-radius': ['case', ['==', ['get', 'selected'], 1], 8, 5],
					'circle-color': [
						'case',
						['==', ['get', 'selected'], 1],
						mapSelected,
						['==', ['get', 'relative'], 1],
						mapRelative,
						mapEdge,
					],
					'circle-stroke-width': 1.2,
					'circle-stroke-color': mapStroke,
				},
			});
			nextMap.on('click', 'editor-locations', async (event) => {
				const id = readLocationId(event);
				if (id === null) return;
				const selectedId = this.hooks.getSelectedId();
				if (
					selectedId !== null &&
					selectedId !== id &&
					(event.originalEvent as MouseEvent).shiftKey
				) {
					await this.hooks.toggleConnection(id);
					return;
				}
				this.hooks.onSelect(id);
			});
			nextMap.on('mouseenter', 'editor-locations', () => {
				canvas.style.cursor = 'pointer';
			});
			nextMap.on('mouseleave', 'editor-locations', () => {
				canvas.style.cursor = '';
			});
			this.setMapData(this.hooks.getLocations(), this.hooks.getSelectedId());
		});

		let dragging = false;
		let dragLat = 0;
		let dragLon = 0;
		nextMap.on('mousedown', 'editor-locations', (event) => {
			const id = readLocationId(event);
			const selectedId = this.hooks.getSelectedId();
			const loc = this.hooks.getSelectedLocation();
			if (id === null || id !== selectedId || !loc) return;
			if ((event.originalEvent as MouseEvent).shiftKey) return;
			dragging = true;
			this.dragTargetId = id;
			this.dragMoved = false;
			dragLat = loc.lat;
			dragLon = loc.lon;
			nextMap.dragPan.disable();
		});
		nextMap.on('mousemove', (event) => {
			// Use dragTargetId (set at mousedown) as the authoritative drag ID.
			// Do NOT use loc.id here: the reactive `loc` may have changed to a
			// different location if the user also triggered a click-selection
			// while the drag was in progress (race #408).
			if (!dragging || this.dragTargetId === null) return;
			dragLat = event.lngLat.lat;
			dragLon = event.lngLat.lng;
			this.dragMoved = true;
			this.setMapData(this.hooks.getLocations(), this.hooks.getSelectedId(), {
				id: this.dragTargetId,
				lat: dragLat,
				lon: dragLon,
			});
		});
		// Listen on window so releasing outside the map canvas still ends the drag.
		this.dragMouseupHandler = async () => {
			if (!dragging) return;
			// Capture dragTargetId before clearing it so the async update targets
			// the correct location even if `loc` has since changed.
			const targetId = this.dragTargetId;
			if (this.dragMoved && targetId !== null) {
				const locations = this.hooks.getLocations();
				await this.hooks.updateLocationById(targetId, (current) =>
					applyDraggedCoordinates(current, locations, dragLat, dragLon),
				);
			}
			dragging = false;
			this.dragTargetId = null;
			this.dragMoved = false;
			nextMap.dragPan.enable();
		};
		window.addEventListener('mouseup', this.dragMouseupHandler);
		this.mapInitializing = false;
	}
}
