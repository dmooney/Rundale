import { beforeEach, describe, it, expect } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { get } from 'svelte/store';
import NpcList from './NpcList.svelte';
import { editorSnapshot, editorSelectedNpcId } from '../../stores/editor';
import type {
	EditorModSnapshot,
	LocationData,
	NpcFileEntry,
} from '$lib/editor-types';

// ── Fixtures ──────────────────────────────────────────────────────────────────

function npc(
	id: number,
	name: string,
	occupation: string,
	home: number,
): NpcFileEntry {
	return {
		id,
		name,
		age: 40,
		occupation,
		personality: '',
		mood: 'calm',
		home,
		workplace: null,
		relationships: [],
		knowledge: [],
	};
}

function loc(id: number, name: string): LocationData {
	return {
		id,
		name,
		description_template: '',
		indoor: false,
		public: true,
		connections: [],
		lat: 53.5,
		lon: -8.1,
		associated_npcs: [],
		aliases: [],
		geo_kind: 'manual',
		relative_to: null,
		geo_source: null,
	};
}

function makeSnapshot(
	npcs: NpcFileEntry[],
	locations: LocationData[],
): EditorModSnapshot {
	return {
		mod_path: '/mods/rundale',
		manifest: {
			id: 'rundale',
			name: 'Rundale',
			title: 'Rundale',
			version: '0.1.0',
			description: 'test',
			start_date: '1822-01-01',
			start_location: 1,
			period_year: 1822,
		},
		npcs: { npcs },
		locations,
		festivals: [],
		encounters: {},
		anachronisms: {
			context_alert_prefix: '',
			context_alert_suffix: '',
			terms: [],
		},
		validation: { errors: [], warnings: [] },
	};
}

// ── Tests ──────────────────────────────────────────────────────────────────────

describe('NpcList (TD-047)', () => {
	beforeEach(() => {
		editorSelectedNpcId.set(null);
		editorSnapshot.set(
			makeSnapshot(
				[npc(1, 'Brigid', 'Spinner', 1), npc(2, 'Seamus', 'Blacksmith', 99)],
				[loc(1, 'The Crossroads')],
			),
		);
	});

	it('renders the NPC count in the header', () => {
		const { getByText } = render(NpcList);
		expect(getByText('NPCs (2)')).toBeTruthy();
	});

	it('renders an item per NPC with occupation and home-location name', () => {
		const { getByText } = render(NpcList);
		expect(getByText('Brigid')).toBeTruthy();
		expect(getByText('Seamus')).toBeTruthy();
		// Brigid's home (id 1) resolves to a name.
		expect(getByText(/Spinner · The Crossroads/)).toBeTruthy();
	});

	it('falls back to "#id" when the home location is unknown', () => {
		const { getByText } = render(NpcList);
		// Seamus's home (id 99) is not in the locations list → "#99".
		expect(getByText(/Blacksmith · #99/)).toBeTruthy();
	});

	it('filters by name or occupation (case-insensitive)', async () => {
		const { getByLabelText, queryByText } = render(NpcList);
		const search = getByLabelText('Search NPCs');
		await fireEvent.input(search, { target: { value: 'blacksmith' } });
		expect(queryByText('Seamus')).toBeTruthy();
		expect(queryByText('Brigid')).toBeNull();
	});

	it('sets the selected NPC id when an item is clicked', async () => {
		const { getByText } = render(NpcList);
		await fireEvent.click(getByText('Seamus'));
		expect(get(editorSelectedNpcId)).toBe(2);
	});
});
