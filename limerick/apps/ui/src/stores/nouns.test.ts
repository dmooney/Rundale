import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { knownNouns } from './nouns';
import { mapData, npcsHere } from './game';
import type { MapData, MapLocation, NpcInfo } from '$lib/types';

// ── Fixtures ──────────────────────────────────────────────────────────────────

function loc(
	name: string,
	opts: { visited?: boolean; adjacent?: boolean } = {},
): MapLocation {
	return {
		id: name.toLowerCase().replace(/\s+/g, '-'),
		name,
		lat: 0,
		lon: 0,
		adjacent: opts.adjacent ?? false,
		hops: opts.adjacent ? 1 : 2,
		visited: opts.visited ?? false,
	};
}

function map(locations: MapLocation[]): MapData {
	return {
		locations,
		edges: [],
		player_location: locations[0]?.id ?? '',
		transport_label: 'on foot',
		transport_id: 'walking',
	};
}

function npc(name: string): NpcInfo {
	return {
		npc_id: name.length,
		name,
		real_name: name,
		occupation: 'Tenant Farmer',
		mood: 'calm',
		introduced: true,
		mood_emoji: '😌',
	};
}

// ── Tests ──────────────────────────────────────────────────────────────────────

describe('knownNouns derived store (TD-050)', () => {
	beforeEach(() => {
		mapData.set(null);
		npcsHere.set([]);
	});

	it('is empty when there is no map and no NPCs', () => {
		expect(get(knownNouns)).toEqual([]);
	});

	it('assigns the documented priorities to locations and NPCs', () => {
		mapData.set(
			map([
				loc('Adjacent Visited', { visited: true, adjacent: true }), // priority 0
				loc('Far Visited', { visited: true, adjacent: false }), // priority 2
				loc('Unexplored', { visited: false }), // priority 3
			]),
		);
		npcsHere.set([npc('Bridget')]); // priority 1

		const nouns = get(knownNouns);
		const byText = Object.fromEntries(nouns.map((n) => [n.text, n]));

		expect(byText['Adjacent Visited'].priority).toBe(0);
		expect(byText['Bridget'].priority).toBe(1);
		expect(byText['Bridget'].category).toBe('npc');
		expect(byText['Far Visited'].priority).toBe(2);
		expect(byText['Unexplored'].priority).toBe(3);
		expect(byText['Adjacent Visited'].category).toBe('location');
	});

	it('sorts by priority ascending (adjacent-visited, npc, far-visited, unexplored)', () => {
		mapData.set(
			map([
				loc('Unexplored', { visited: false }),
				loc('Far Visited', { visited: true, adjacent: false }),
				loc('Adjacent Visited', { visited: true, adjacent: true }),
			]),
		);
		npcsHere.set([npc('Seamus')]);

		const order = get(knownNouns).map((n) => n.text);
		expect(order).toEqual([
			'Adjacent Visited', // 0
			'Seamus', // 1
			'Far Visited', // 2
			'Unexplored', // 3
		]);
	});

	it('breaks ties alphabetically within the same priority', () => {
		mapData.set(
			map([
				loc('Zebra Field', { visited: true, adjacent: true }),
				loc('Apple Orchard', { visited: true, adjacent: true }),
				loc('Mill Pond', { visited: true, adjacent: true }),
			]),
		);

		const order = get(knownNouns).map((n) => n.text);
		expect(order).toEqual(['Apple Orchard', 'Mill Pond', 'Zebra Field']);
	});

	it('orders NPCs alphabetically among themselves (all priority 1)', () => {
		mapData.set(null);
		npcsHere.set([npc('Tadhg'), npc('Aoife'), npc('Niamh')]);

		const order = get(knownNouns).map((n) => n.text);
		expect(order).toEqual(['Aoife', 'Niamh', 'Tadhg']);
	});
});
