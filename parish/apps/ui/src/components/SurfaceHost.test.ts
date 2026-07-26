import { fireEvent, render, waitFor } from '@testing-library/svelte';
import { get } from 'svelte/store';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { debugSnapshot } from '../stores/debug';
import { uiConfig } from '../stores/game';
import {
	activeSurface,
	closeSurface,
	openSurface,
	resetSurfaceCoordinatorForTests,
} from '../stores/surfaceCoordinator';
import SurfaceHost from './SurfaceHost.svelte';

vi.mock('$lib/screenshot', () => ({
	captureScreen: vi.fn(async () => 'data:image/png;base64,fresh-chat'),
}));

vi.mock('$lib/map/controller', () => ({
	MapController: class {
		onLocationClick() {}
		onLocationHover() {}
		updateMap() {}
		fitBounds() {}
		setTileSource() {}
		startTravel() {}
		stopTravel() {}
		destroy() {}
	},
}));

describe('SurfaceHost', () => {
	beforeEach(() => {
		resetSurfaceCoordinatorForTests();
		uiConfig.update((config) => ({ ...config, base_mod_required: false }));
		debugSnapshot.set(null);
	});

	it('renders no surface by default', () => {
		const { queryByTestId } = render(SurfaceHost);
		expect(queryByTestId('surface-backdrop')).toBeNull();
	});

	it('renders map and debug as labelled dialogs', async () => {
		const { getByRole } = render(SurfaceHost);

		await openSurface('debug', null);
		expect(
			await waitFor(() => getByRole('dialog', { name: 'Debug records' })),
		).toBeTruthy();

		closeSurface('debug', { restoreFocus: false });
		await openSurface('map', null);
		expect(
			await waitFor(() => getByRole('dialog', { name: 'Parish map' })),
		).toBeTruthy();
	});

	it('closes a coordinated surface with Escape', async () => {
		const { getByTestId } = render(SurfaceHost);
		await openSurface('debug', null);
		await waitFor(() => expect(getByTestId('surface-debug')).toBeTruthy());

		await fireEvent.keyDown(window, { key: 'Escape' });
		expect(get(activeSurface)).toBeNull();
	});

	it('traps focus inside the active surface', async () => {
		const { getByRole } = render(SurfaceHost);
		await openSurface('debug', null);
		const close = await waitFor(() =>
			getByRole('button', { name: 'Close Debug records' }),
		);
		await waitFor(() => expect(document.activeElement).toBe(close));

		document.body.tabIndex = -1;
		document.body.focus();
		await fireEvent.keyDown(window, { key: 'Tab' });
		expect(document.activeElement).toBe(close);
		document.body.removeAttribute('tabindex');
	});
});
