import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
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
	game_epoch_ms: morningEpoch(),
	speed_factor: 0,
	name_hints: []
};

describe('StatusBar', () => {
	beforeEach(() => {
		worldState.set(null);
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

	it('renders the clock element', () => {
		worldState.set(snapshot);
		const { container } = render(StatusBar);
		const clock = container.querySelector('.clock');
		expect(clock).toBeTruthy();
	});
});
