import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { mapData } from '../stores/game';
import FullMapOverlay from './FullMapOverlay.svelte';

// MapLibre GL JS requires WebGL, which jsdom doesn't provide. Mock the
// module so FullMapOverlay mounts without trying to create a real map.
vi.mock('maplibre-gl');

// Mock the IPC layer used by onLocationClick.
vi.mock('$lib/ipc', () => ({
	submitInput: vi.fn(() => Promise.resolve()),
}));

// Spy on MapController.fitBounds via the module mock so we can count calls
// without depending on the real MapLibre instance.
const fitBoundsSpy = vi.fn();

vi.mock('$lib/map/controller', () => {
	class FakeMapController {
		onLocationClick() {}
		onLocationHover() {}
		updateMap() {}
		fitBounds(...args: unknown[]) {
			fitBoundsSpy(...args);
		}
		setTileSource() {}
		startTravel() {}
		stopTravel() {}
		destroy() {}
	}
	return { MapController: FakeMapController };
});

const testMap = {
	locations: [
		{
			id: 'loc1',
			name: 'Kilteevan',
			lat: 53.8,
			lon: -8.15,
			adjacent: false,
			hops: 0,
		},
		{
			id: 'loc2',
			name: 'Roscommon',
			lat: 53.63,
			lon: -8.19,
			adjacent: true,
			hops: 1,
		},
	],
	edges: [['loc1', 'loc2']] as [string, string][],
	player_location: 'loc1',
	player_lat: 53.8,
	player_lon: -8.15,
};

describe('FullMapOverlay', () => {
	beforeEach(() => {
		mapData.set(null);
		fitBoundsSpy.mockClear();
	});

	it('renders the map container', () => {
		const { container } = render(FullMapOverlay, {
			props: { onclose: vi.fn() },
		});
		expect(container.querySelector('.map-container')).toBeTruthy();
	});

	it('renders the close button', () => {
		const { container } = render(FullMapOverlay, {
			props: { onclose: vi.fn() },
		});
		expect(container.querySelector('.close-btn')).toBeTruthy();
	});

	it('calls fitBounds exactly once when map data is already present at mount', () => {
		// Set map data BEFORE rendering so onMount sees it immediately.
		// This is the bug scenario: without the fix, onMount calls fitBounds,
		// then the $effect fires with hasFitOnce still false and calls it again.
		mapData.set(testMap);
		render(FullMapOverlay, { props: { onclose: vi.fn() } });
		expect(fitBoundsSpy).toHaveBeenCalledTimes(1);
	});

	it('calls fitBounds once when map data arrives after mount', () => {
		// No map data at mount — onMount skips fitBounds.
		render(FullMapOverlay, { props: { onclose: vi.fn() } });
		expect(fitBoundsSpy).toHaveBeenCalledTimes(0);

		// Populate map data and flush pending effects — the $effect should
		// call fitBounds exactly once.
		flushSync(() => {
			mapData.set(testMap);
		});
		expect(fitBoundsSpy).toHaveBeenCalledTimes(1);
	});

	it('does not call fitBounds again on subsequent map data updates', () => {
		mapData.set(testMap);
		render(FullMapOverlay, { props: { onclose: vi.fn() } });
		expect(fitBoundsSpy).toHaveBeenCalledTimes(1);

		// Simulate a map update (new location added). fitBounds should not fire again.
		flushSync(() => {
			mapData.set({
				...testMap,
				locations: [
					...testMap.locations,
					{
						id: 'loc3',
						name: 'Strokestown',
						lat: 53.77,
						lon: -8.1,
						adjacent: false,
						hops: 2,
					},
				],
			});
		});
		expect(fitBoundsSpy).toHaveBeenCalledTimes(1);
	});
});
