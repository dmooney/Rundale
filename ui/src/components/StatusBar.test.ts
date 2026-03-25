import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { worldState } from '../stores/game';
import StatusBar from './StatusBar.svelte';
import type { WorldSnapshot } from '$lib/types';

const snapshot: WorldSnapshot = {
	location_name: 'Baile Átha Cliath',
	location_description: 'A bustling city.',
	time_label: 'Morning',
	hour: 8,
	weather: 'Overcast',
	season: 'Spring',
	festival: null,
	paused: false
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

	it('shows time label and hour', () => {
		worldState.set(snapshot);
		const { getByText } = render(StatusBar);
		expect(getByText('Morning 08:00')).toBeTruthy();
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
		expect(getByText('🎉 Samhain')).toBeTruthy();
	});

	it('shows paused indicator when paused', () => {
		worldState.set({ ...snapshot, paused: true });
		const { getByText } = render(StatusBar);
		expect(getByText('⏸ Paused')).toBeTruthy();
	});
});
