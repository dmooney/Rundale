import { beforeEach, describe, it, expect } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { get } from 'svelte/store';
import LocationList from './LocationList.svelte';
import { editorSnapshot, editorSelectedLocationId } from '../../stores/editor';
import type { EditorModSnapshot, LocationData } from '$lib/editor-types';

// ── Fixtures ──────────────────────────────────────────────────────────────────

function loc(
	id: number,
	name: string,
	overrides: Partial<LocationData> = {},
): LocationData {
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
		...overrides,
	};
}

function makeSnapshot(locations: LocationData[]): EditorModSnapshot {
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
		npcs: { npcs: [] },
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

describe('LocationList (TD-047)', () => {
	beforeEach(() => {
		editorSelectedLocationId.set(null);
		editorSnapshot.set(
			makeSnapshot([
				loc(1, 'The Crossroads', {
					connections: [{ target: 2, path_description: 'a path' }],
				}),
				loc(2, 'The Forge', { indoor: true, public: false }),
			]),
		);
	});

	it('renders the location count in the header', () => {
		const { getByText } = render(LocationList);
		expect(getByText('Locations (2)')).toBeTruthy();
	});

	it('renders an item per location with its connection count and flags', () => {
		const { getByText } = render(LocationList);
		expect(getByText('The Crossroads')).toBeTruthy();
		expect(getByText('The Forge')).toBeTruthy();
		// The Forge is indoor + private — its meta line reflects both.
		expect(getByText(/1 connections/)).toBeTruthy();
		expect(getByText(/indoor/)).toBeTruthy();
		expect(getByText(/private/)).toBeTruthy();
	});

	it('filters the list by the search box (case-insensitive)', async () => {
		const { getByLabelText, queryByText } = render(LocationList);
		const search = getByLabelText('Search locations');
		await fireEvent.input(search, { target: { value: 'forge' } });
		expect(queryByText('The Forge')).toBeTruthy();
		expect(queryByText('The Crossroads')).toBeNull();
	});

	it('sets the selected location id when an item is clicked', async () => {
		const { getByText } = render(LocationList);
		await fireEvent.click(getByText('The Forge'));
		expect(get(editorSelectedLocationId)).toBe(2);
	});
});
