import { describe, it, expect, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import { mapData } from '../stores/game';
import MapPanel from './MapPanel.svelte';

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

	it('renders correct number of node circles for nearby locations', () => {
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

	it('only shows locations within hop radius', () => {
		// Add a distant location (hops = 5, beyond MINIMAP_HOP_RADIUS of 3)
		const mapWithDistant = {
			...testMap,
			locations: [
				...testMap.locations,
				{ id: 'loc3', name: 'Galway', lat: 53.27, lon: -9.05, adjacent: false, hops: 5 }
			]
		};
		mapData.set(mapWithDistant);
		const { container } = render(MapPanel);
		const circles = container.querySelectorAll('circle');
		// Only loc1 (hops=0) and loc2 (hops=1) should be rendered as circles
		expect(circles.length).toBe(2);
	});

	it('shows expand button', () => {
		mapData.set(testMap);
		const { container } = render(MapPanel);
		const expandBtn = container.querySelector('.expand-btn');
		expect(expandBtn).toBeTruthy();
	});
});
