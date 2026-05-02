import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { tick } from 'svelte';
import { worldState } from '../stores/game';
import StatusBar from './StatusBar.svelte';
import type { WorldSnapshot } from '$lib/types';

/**
 * Build a UTC epoch for 08:00 today so the StatusBar's rAF-driven clock
 * computes the correct time. speed_factor=0 freezes the interpolation.
 */
function morningEpoch(): number {
	const d = new Date();
	d.setUTCHours(8, 0, 0, 0);
	return d.getTime();
}

const snapshot: WorldSnapshot = {
	location_name: 'Baile Átha Cliath',
	location_description: 'A bustling city.',
	time_label: 'Morning',
	hour: 8,
	minute: 0,
	weather: 'Overcast',
	season: 'Spring',
	festival: null,
	paused: false,
	inference_paused: false,
	game_epoch_ms: morningEpoch(),
	speed_factor: 0,
	name_hints: [],
	day_of_week: 'Monday'
};

describe('StatusBar', () => {
	beforeEach(() => {
		worldState.set(null);
	});

	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('shows loading when no world state', () => {
		const { getByText } = render(StatusBar);
		expect(getByText('Loading…')).toBeTruthy();
	});

	it('shows location name from world state', () => {
		worldState.set(snapshot);
		const { getByText } = render(StatusBar);
		expect(getByText('Baile Átha Cliath')).toBeTruthy();
	});

	it('shows weather and season', () => {
		worldState.set(snapshot);
		const { getByText } = render(StatusBar);
		expect(getByText('Overcast')).toBeTruthy();
		expect(getByText('Spring')).toBeTruthy();
	});

	it('shows festival badge when festival is set', () => {
		worldState.set({ ...snapshot, festival: 'Samhain' });
		const { getByText } = render(StatusBar);
		expect(getByText('✦ Samhain')).toBeTruthy();
	});

	it('shows paused indicator when paused', () => {
		worldState.set({ ...snapshot, paused: true });
		const { getByText } = render(StatusBar);
		expect(getByText('⏸ Paused')).toBeTruthy();
	});

	it('does not show paused indicator when only inference is paused', () => {
		worldState.set({ ...snapshot, inference_paused: true });
		const { queryByText } = render(StatusBar);
		expect(queryByText('⏸ Paused')).toBeNull();
	});

	it('renders the clock element', () => {
		worldState.set(snapshot);
		const { container } = render(StatusBar);
		const clock = container.querySelector('.clock');
		expect(clock).toBeTruthy();
	});

	it('does not schedule another rAF frame after clockFrozen becomes true', async () => {
		// Capture the rAF callback so we can invoke it manually.
		let capturedCb: FrameRequestCallback | null = null;
		const rafSpy = vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
			capturedCb = cb;
			return 1;
		});

		// Start with an unfrozen clock so the rAF loop is running.
		worldState.set(snapshot); // paused: false
		render(StatusBar);
		await tick();

		// Freeze the clock.
		worldState.set({ ...snapshot, paused: true });
		await tick();

		// Clear any rAF calls made while transitioning to frozen.
		rafSpy.mockClear();

		// Simulate the next animation frame firing while the clock is frozen.
		// tick() should bail immediately without scheduling another frame.
		expect(capturedCb).not.toBeNull();
		capturedCb!(performance.now());

		expect(rafSpy).not.toHaveBeenCalled();
	});
});
