import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { mapData } from '../stores/game';
import MapPanel from './MapPanel.svelte';

const testMap = {
	locations: [
		{ id: 'loc1', name: 'Dublin', lat: 53.35, lon: -6.26, adjacent: false },
		{ id: 'loc2', name: 'Howth', lat: 53.39, lon: -6.07, adjacent: true }
	],
	edges: [['loc1', 'loc2']] as [string, string][],
	player_location: 'loc1'
};

describe('MapPanel', () => {
	beforeEach(() => {
		mapData.set(null);
	});

	it('shows loading when no map data', () => {
		const { getByText } = render(MapPanel);
		expect(getByText('Loading map…')).toBeTruthy();
	});

	it('renders SVG when map data is available', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		expect(container.querySelector('svg')).toBeTruthy();
	});

	it('renders correct number of node circles', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		const circles = container.querySelectorAll('circle');
		expect(circles.length).toBe(2);
	});

	it('renders edge lines', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		const lines = container.querySelectorAll('line.edge');
		expect(lines.length).toBe(1);
	});

	it('shows player node with accent fill', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		const playerGroup = container.querySelector('.node.player');
		expect(playerGroup).toBeTruthy();
	});

	it('marks adjacent nodes', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		const adjacent = container.querySelector('.node.adjacent');
		expect(adjacent).toBeTruthy();
	});
});
