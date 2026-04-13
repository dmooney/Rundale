import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { mapData, fullMapOpen } from '../stores/game';
import MapPanel from './MapPanel.svelte';

// MapLibre GL JS requires WebGL, which jsdom doesn't provide. Mock the
// module so the MapPanel mounts without trying to create a real map.
vi.mock('maplibre-gl', () => {
	class FakeMap {
		on() {}
		remove() {}
		getCanvas() {
			const el = document.createElement('canvas');
			return el as HTMLCanvasElement;
		}
		getSource() {
			return undefined;
		}
		setStyle() {}
		project() {
			return { x: 0, y: 0 };
		}
		jumpTo() {}
		easeTo() {}
		fitBounds() {}
		addControl() {}
		removeControl() {}
	}
	class FakeMarker {
		setLngLat() {
			return this;
		}
		addTo() {
			return this;
		}
		remove() {}
	}
	class FakeLngLatBounds {
		extend() {
			return this;
		}
	}
	const def = { Map: FakeMap, Marker: FakeMarker, LngLatBounds: FakeLngLatBounds };
	return {
		default: def,
		Map: FakeMap,
		Marker: FakeMarker,
		LngLatBounds: FakeLngLatBounds
	};
});

const testMap = {
	locations: [
		{ id: 'loc1', name: 'Dublin', lat: 53.35, lon: -6.26, adjacent: false, hops: 0 },
		{ id: 'loc2', name: 'Howth', lat: 53.39, lon: -6.07, adjacent: true, hops: 1 }
	],
	edges: [['loc1', 'loc2']] as [string, string][],
	player_location: 'loc1',
	player_lat: 53.35,
	player_lon: -6.26
};

describe('MapPanel', () => {
	beforeEach(() => {
		mapData.set(null);
		fullMapOpen.set(false);
	});

	it('shows loading when no map data', () => {
		const { getByText } = render(MapPanel);
		expect(getByText('Loading map…')).toBeTruthy();
	});

	it('renders the map container when map data is available', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		expect(container.querySelector('.map-container')).toBeTruthy();
	});

	it('shows the expand button', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		expect(container.querySelector('.expand-btn')).toBeTruthy();
	});

	it('clicking the expand button opens the full map overlay', async () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		const btn = container.querySelector('.expand-btn') as HTMLButtonElement;
		expect(btn).toBeTruthy();
		btn.click();
		expect(get(fullMapOpen)).toBe(true);
	});

	it('shows the map panel wrapper element', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		expect(container.querySelector('[data-testid="map-panel"]')).toBeTruthy();
	});
});
