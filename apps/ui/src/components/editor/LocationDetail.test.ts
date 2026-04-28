import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';

import LocationDetail from './LocationDetail.svelte';
import {
	editorDirty,
	editorSelectedLocationId,
	editorSnapshot,
	editorValidation
} from '../../stores/editor';
import type { EditorModSnapshot } from '$lib/editor-types';

interface MapHarness {
	getCanvas(): { style: { cursor: string } };
	trigger(event: string, payload?: unknown, layer?: string): Promise<void>;
}

const mockState = vi.hoisted(() => ({
	mapConstructCount: 0,
	mapRemoveCount: 0,
	lastMap: null as MapHarness | null,
	lastMapOptions: null as Record<string, unknown> | null,
	editorUpdateLocationsMock: vi.fn(async () => ({ errors: [], warnings: [] })),
	editorSaveMock: vi.fn(async () => ({ saved: true, validation: { errors: [], warnings: [] } }))
}));

vi.mock('maplibre-gl', () => {
	class FakeGeoJSONSource {
		setData() {}
	}

	class FakeMap {
		private sources = new Map<string, FakeGeoJSONSource>();
		private handlers = new Map<string, Array<(...args: unknown[]) => unknown>>();
		private canvas = { style: { cursor: '' } };
		dragPan = {
			disable() {},
			enable() {}
		};

		constructor(options: Record<string, unknown>) {
			mockState.mapConstructCount += 1;
			mockState.lastMap = this;
			mockState.lastMapOptions = options;
		}

		on(event: string, layerOrCb: string | ((...args: unknown[]) => unknown), maybeCb?: (...args: unknown[]) => unknown) {
			const layer = typeof layerOrCb === 'string' ? layerOrCb : '';
			const cb = typeof layerOrCb === 'function' ? layerOrCb : maybeCb;
			if (!cb) return;
			if (event === 'load' && !layer) {
				queueMicrotask(() => cb());
				return;
			}
			const key = `${event}:${layer}`;
			this.handlers.set(key, [...(this.handlers.get(key) ?? []), cb]);
		}

		addControl() {}

		addSource(id: string) {
			this.sources.set(id, new FakeGeoJSONSource());
		}

		addLayer() {}

		getSource(id: string) {
			return this.sources.get(id);
		}

		getCanvas() {
			return this.canvas;
		}

		async trigger(event: string, payload: unknown = {}, layer = '') {
			for (const cb of this.handlers.get(`${event}:${layer}`) ?? []) {
				await cb(payload);
			}
		}

		easeTo() {}

		off() {}

		remove() {
			mockState.mapRemoveCount += 1;
		}
	}

	class FakeNavigationControl {}

	return {
		default: {
			Map: FakeMap,
			NavigationControl: FakeNavigationControl
		},
		Map: FakeMap,
		NavigationControl: FakeNavigationControl
	};
});

vi.mock('$lib/ipc', () => ({
	getUiConfig: vi.fn(async () => ({
		active_tile_source: 'osm',
		tile_sources: [
			{
				id: 'osm',
				label: 'OpenStreetMap',
				url: 'https://tile.openstreetmap.org/{z}/{x}/{y}.png',
				attribution: 'OSM',
				tile_size: 256,
				minzoom: 0,
				maxzoom: 19,
				tms: false,
				raster_opacity: 1,
				raster_saturation: 0
			}
		]
	}))
}));

vi.mock('$lib/editor-ipc', () => ({
	editorUpdateLocations: mockState.editorUpdateLocationsMock,
	editorSave: mockState.editorSaveMock
}));

function snapshot(): EditorModSnapshot {
	return {
		mod_path: '/mods/rundale',
		manifest: {
			id: 'rundale',
			name: 'Rundale',
			title: 'Rundale',
			version: '0.1.0',
			description: 'test',
			start_date: '1822-01-01',
			start_location: 1,
			period_year: 1822
		},
		npcs: { npcs: [] },
		locations: [
			{
				id: 1,
				name: 'The Crossroads',
				description_template: '',
				indoor: false,
				public: true,
				connections: [],
				lat: 53.5,
				lon: -8.1,
				associated_npcs: [],
				aliases: [],
				geo_kind: 'manual',
				relative_to: null,
				geo_source: null
			},
			{
				id: 2,
				name: 'The Mill',
				description_template: '',
				indoor: false,
				public: true,
				connections: [],
				lat: 53.51,
				lon: -8.11,
				associated_npcs: [],
				aliases: [],
				geo_kind: 'manual',
				relative_to: null,
				geo_source: null
			}
		],
		festivals: [],
		encounters: {},
		anachronisms: {
			context_alert_prefix: '',
			context_alert_suffix: '',
			terms: []
		},
		validation: { errors: [], warnings: [] }
	};
}

describe('LocationDetail', () => {
	beforeEach(() => {
		mockState.mapConstructCount = 0;
		mockState.mapRemoveCount = 0;
		mockState.lastMap = null;
		mockState.lastMapOptions = null;
		mockState.editorUpdateLocationsMock.mockClear();
		mockState.editorSaveMock.mockClear();
		editorSnapshot.set(snapshot());
		editorSelectedLocationId.set(null);
		editorDirty.set(false);
		editorValidation.set({ errors: [], warnings: [] });
	});

	it('creates the map when a location is selected after the component mounts', async () => {
		render(LocationDetail);
		expect(mockState.mapConstructCount).toBe(0);

		editorSelectedLocationId.set(1);

		await waitFor(() => {
			expect(mockState.mapConstructCount).toBe(1);
		});
		expect(mockState.lastMapOptions?.boxZoom).toBe(false);
	});

	it('removes the map when the selected location is cleared', async () => {
		editorSelectedLocationId.set(1);
		render(LocationDetail);

		await waitFor(() => {
			expect(mockState.mapConstructCount).toBe(1);
		});

		editorSelectedLocationId.set(null);

		await waitFor(() => {
			expect(mockState.mapRemoveCount).toBe(1);
		});
	});

	it('shift-click toggles a bidirectional connection to the clicked location', async () => {
		editorSelectedLocationId.set(1);
		render(LocationDetail);

		await waitFor(() => {
			expect(mockState.lastMap).not.toBeNull();
		});

		await mockState.lastMap!.trigger(
			'click',
			{
				features: [{ properties: { id: 2 } }],
				originalEvent: { shiftKey: true }
			},
			'editor-locations'
		);

		await waitFor(() => {
			expect(mockState.editorUpdateLocationsMock).toHaveBeenCalledTimes(1);
		});

		const calls = mockState.editorUpdateLocationsMock.mock.calls as unknown as Array<[unknown]>;
		const updatedLocations = calls.at(-1)?.[0];
		if (!updatedLocations) throw new Error('expected editorUpdateLocations to be called');
		const locations = updatedLocations as Array<{
			id: number;
			connections: Array<{ target: number; path_description: string }>;
		}>;
		expect(locations.find((loc) => loc.id === 1)?.connections).toEqual([
			{ target: 2, path_description: 'an old lane between settlements' }
		]);
		expect(locations.find((loc) => loc.id === 2)?.connections).toEqual([
			{ target: 1, path_description: 'an old lane between settlements' }
		]);
	});

	it('shows a pointer cursor when hovering a map point', async () => {
		editorSelectedLocationId.set(1);
		render(LocationDetail);

		await waitFor(() => {
			expect(mockState.lastMap).not.toBeNull();
		});

		expect(mockState.lastMap!.getCanvas().style.cursor).toBe('');
		await mockState.lastMap!.trigger('mouseenter', {}, 'editor-locations');
		expect(mockState.lastMap!.getCanvas().style.cursor).toBe('pointer');
		await mockState.lastMap!.trigger('mouseleave', {}, 'editor-locations');
		expect(mockState.lastMap!.getCanvas().style.cursor).toBe('');
	});

	// #408 — mousedown on non-selected location must not initiate drag
	it('does not start a drag when mousedown fires on a non-selected location', async () => {
		editorSelectedLocationId.set(1);
		render(LocationDetail);

		await waitFor(() => {
			expect(mockState.lastMap).not.toBeNull();
		});

		const map = mockState.lastMap!;

		// Mousedown on location 2 (not the currently selected location 1)
		await map.trigger(
			'mousedown',
			{ features: [{ properties: { id: 2 } }], originalEvent: { shiftKey: false } },
			'editor-locations'
		);
		// A move should not trigger any update because drag was never started
		await map.trigger('mousemove', { lngLat: { lat: 53.52, lng: -8.12 } });
		window.dispatchEvent(new MouseEvent('mouseup'));

		expect(mockState.editorUpdateLocationsMock).not.toHaveBeenCalled();
	});

	// #408 — if selection changes during an active drag, mouseup must update
	// the originally dragged location, not the newly selected one.
	it('commits drag to the originally dragged location even if selection changes mid-drag', async () => {
		editorSelectedLocationId.set(1);
		render(LocationDetail);

		await waitFor(() => {
			expect(mockState.lastMap).not.toBeNull();
		});

		const map = mockState.lastMap!;

		// Start dragging location 1 (the selected one)
		await map.trigger(
			'mousedown',
			{ features: [{ properties: { id: 1 } }], originalEvent: { shiftKey: false } },
			'editor-locations'
		);
		// Move the mouse — drag is in progress
		await map.trigger('mousemove', { lngLat: { lat: 53.60, lng: -8.20 } });

		// Simulate selection change mid-drag (click race) — location 2 becomes selected
		editorSelectedLocationId.set(2);
		await waitFor(() => {}); // let Svelte flush reactivity

		// Release mouse — should commit to location 1 (the dragged one), not location 2
		window.dispatchEvent(new MouseEvent('mouseup'));

		await waitFor(() => {
			expect(mockState.editorUpdateLocationsMock).toHaveBeenCalledTimes(1);
		});

		const calls = mockState.editorUpdateLocationsMock.mock.calls as unknown as Array<[unknown]>;
		const updatedLocations = calls.at(-1)?.[0] as Array<{ id: number; lat: number; lon: number }>;
		// Location 1 coordinates should be updated
		const loc1 = updatedLocations.find((l) => l.id === 1);
		expect(loc1).toBeDefined();
		expect(loc1!.lat).toBeCloseTo(53.60, 4);
		// Location 2 should be unchanged
		const loc2 = updatedLocations.find((l) => l.id === 2);
		expect(loc2?.lat).toBeCloseTo(53.51, 4);
	});
});
