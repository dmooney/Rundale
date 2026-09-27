import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { mapData } from '../stores/game';
import FullMapOverlay from './FullMapOverlay.svelte';
import { submitInput } from '$lib/ipc';

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
const moveUnsubscribeSpy = vi.fn();
let locationClickHandler:
	((info: { id: string; name: string; adjacent: boolean }) => void) | undefined;

vi.mock('$lib/map/controller', () => {
	class FakeMapController {
		onLocationClick(
			handler: (info: { id: string; name: string; adjacent: boolean }) => void,
		) {
			locationClickHandler = handler;
		}
		onLocationHover() {}
		updateMap() {}
		fitBounds(...args: unknown[]) {
			fitBoundsSpy(...args);
		}
		setTileSource() {}
		projectToScreen(lat: number, lon: number) {
			return { x: lon + 200, y: lat + 100 };
		}
		addMoveListener() {
			return moveUnsubscribeSpy;
		}
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
	transport_label: 'on foot',
	transport_id: 'walking',
};

describe('FullMapOverlay', () => {
	beforeEach(() => {
		mapData.set(null);
		fitBoundsSpy.mockClear();
		moveUnsubscribeSpy.mockClear();
		locationClickHandler = undefined;
		vi.mocked(submitInput).mockClear();
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

	it('explains the geographic pointer and zoom loop', () => {
		const { getByLabelText } = render(FullMapOverlay, {
			props: { onclose: vi.fn() },
		});
		expect(getByLabelText('Map controls')).toHaveTextContent(
			'Drag to explore · scroll or pinch to zoom · click an outlined place to travel',
		);
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

	it('exposes only adjacent places as native travel controls', () => {
		mapData.set(testMap);
		const { getByRole, queryByRole } = render(FullMapOverlay, {
			props: { onclose: vi.fn() },
		});

		expect(getByRole('button', { name: 'Travel to Roscommon' })).toBeVisible();
		expect(
			queryByRole('button', { name: 'Travel to Kilteevan' }),
		).not.toBeInTheDocument();
	});

	it('submits travel on one native pointer activation', async () => {
		mapData.set(testMap);
		const { getByRole } = render(FullMapOverlay, {
			props: { onclose: vi.fn() },
		});

		await fireEvent.click(getByRole('button', { name: 'Travel to Roscommon' }));

		await waitFor(() =>
			expect(submitInput).toHaveBeenCalledExactlyOnceWith('go to Roscommon'),
		);
	});

	it('retains MapLibre activation as a fallback without allowing remote travel', async () => {
		mapData.set(testMap);
		render(FullMapOverlay, { props: { onclose: vi.fn() } });

		locationClickHandler?.({
			id: 'loc1',
			name: 'Kilteevan',
			adjacent: false,
		});
		locationClickHandler?.({
			id: 'loc2',
			name: 'Roscommon',
			adjacent: true,
		});

		await waitFor(() =>
			expect(submitInput).toHaveBeenCalledExactlyOnceWith('go to Roscommon'),
		);
	});

	it('releases the map-position subscription when closed', () => {
		mapData.set(testMap);
		const view = render(FullMapOverlay, { props: { onclose: vi.fn() } });
		view.unmount();
		expect(moveUnsubscribeSpy).toHaveBeenCalledOnce();
	});
});
