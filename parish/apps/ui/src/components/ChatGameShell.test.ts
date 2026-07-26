import { render } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { focailOpen } from '../stores/game';
import { resetSurfaceCoordinatorForTests } from '../stores/surfaceCoordinator';
import ChatGameShell from './ChatGameShell.svelte';

vi.mock('$lib/map/controller', () => ({
	MapController: class {
		onLocationClick() {}
		onLocationHover() {}
		addMoveListener() {
			return () => {};
		}
		getContainerSize() {
			return { width: 0, height: 0 };
		}
		projectToScreen() {
			return { x: 0, y: 0 };
		}
		updateMap() {}
		fitBounds() {}
		setTileSource() {}
		startTravel() {}
		stopTravel() {}
		destroy() {}
	},
}));

describe('ChatGameShell', () => {
	beforeEach(() => {
		focailOpen.set(false);
		resetSurfaceCoordinatorForTests();
	});

	it('renders the mature chat interaction components', () => {
		const { getByTestId } = render(ChatGameShell);

		expect(getByTestId('status-bar')).toBeTruthy();
		expect(getByTestId('chat-panel')).toBeTruthy();
		expect(getByTestId('input-field')).toBeTruthy();
		expect(getByTestId('sidebar')).toBeTruthy();
	});

	it('exposes semantic mobile panel controls', () => {
		const { getByRole } = render(ChatGameShell);
		expect(getByRole('button', { name: 'Toggle parish map' })).toBeTruthy();
		expect(
			getByRole('button', {
				name: 'Toggle nearby people and language hints',
			}),
		).toBeTruthy();
	});
});
