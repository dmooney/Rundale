import { beforeEach, describe, it, expect, vi } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import NpcDetail from './NpcDetail.svelte';
import {
	editorSnapshot,
	editorSelectedNpcId,
	editorDirty,
	editorValidation,
} from '../../stores/editor';
import type { EditorModSnapshot, NpcFileEntry } from '$lib/editor-types';

const mocks = vi.hoisted(() => ({
	editorUpdateNpcs: vi.fn(async () => ({ errors: [], warnings: [] })),
	editorSave: vi.fn(async () => ({
		saved: true,
		validation: { errors: [], warnings: [] },
	})),
}));

vi.mock('$lib/editor-ipc', () => ({
	editorUpdateNpcs: mocks.editorUpdateNpcs,
	editorSave: mocks.editorSave,
}));

// ── Fixture ───────────────────────────────────────────────────────────────────

const brigid: NpcFileEntry = {
	id: 1,
	name: 'Brigid Ní Fhaoláin',
	age: 34,
	occupation: 'Spinner',
	personality: 'Warm and stubborn.',
	mood: 'content',
	home: 1,
	workplace: null,
	relationships: [],
	knowledge: ['Knows the local herb lore'],
};

const seamus: NpcFileEntry = {
	id: 2,
	name: 'Séamus Ó Briain',
	age: 55,
	occupation: 'Blacksmith',
	personality: 'Gruff.',
	mood: 'tired',
	home: 2,
	workplace: 2,
	relationships: [{ target_id: 1, kind: 'friend', strength: 0.7 }],
	knowledge: [],
};

function makeSnapshot(): EditorModSnapshot {
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
		npcs: { npcs: [brigid, seamus] },
		locations: [
			{
				id: 1,
				name: 'The Crossroads',
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
			},
			{
				id: 2,
				name: 'The Forge',
				description_template: '',
				indoor: true,
				public: false,
				connections: [],
				lat: 53.51,
				lon: -8.11,
				associated_npcs: [],
				aliases: [],
				geo_kind: 'manual',
				relative_to: null,
				geo_source: null,
			},
		],
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

// ── Tests ─────────────────────────────────────────────────────────────────────

describe('NpcDetail', () => {
	beforeEach(() => {
		editorSnapshot.set(makeSnapshot());
		editorSelectedNpcId.set(null);
		editorDirty.set(false);
		editorValidation.set(null);
		mocks.editorUpdateNpcs.mockClear();
		mocks.editorSave.mockClear();
	});

	it('shows the empty-state prompt when no NPC is selected', () => {
		const { getByText } = render(NpcDetail);
		expect(getByText('Select an NPC from the list to edit.')).toBeTruthy();
	});

	it('renders the NPC name in the header when an NPC is selected', () => {
		editorSelectedNpcId.set(1);
		const { getByText } = render(NpcDetail);
		expect(getByText('Brigid Ní Fhaoláin')).toBeTruthy();
	});

	it('shows the occupation field populated with the selected NPC value', () => {
		editorSelectedNpcId.set(1);
		const { getByDisplayValue } = render(NpcDetail);
		expect(getByDisplayValue('Spinner')).toBeTruthy();
	});

	it('shows the mood field populated with the selected NPC value', () => {
		editorSelectedNpcId.set(1);
		const { getByDisplayValue } = render(NpcDetail);
		expect(getByDisplayValue('content')).toBeTruthy();
	});

	it('renders a "Save NPCs" button when an NPC is selected', () => {
		editorSelectedNpcId.set(1);
		const { getByText } = render(NpcDetail);
		expect(getByText('Save NPCs')).toBeTruthy();
	});

	it('Save NPCs button is disabled when editorDirty is false', () => {
		editorSelectedNpcId.set(1);
		editorDirty.set(false);
		const { getByText } = render(NpcDetail);
		const btn = getByText('Save NPCs') as HTMLButtonElement;
		expect(btn.disabled).toBe(true);
	});

	it('Save NPCs button is enabled when editorDirty is true', () => {
		editorSelectedNpcId.set(1);
		editorDirty.set(true);
		const { getByText } = render(NpcDetail);
		const btn = getByText('Save NPCs') as HTMLButtonElement;
		expect(btn.disabled).toBe(false);
	});

	it('shows relationships count section', () => {
		editorSelectedNpcId.set(2);
		const { getByText } = render(NpcDetail);
		// seamus has 1 relationship
		expect(getByText('Relationships (1)')).toBeTruthy();
	});

	it('shows "No relationships defined." when NPC has no relationships', () => {
		editorSelectedNpcId.set(1);
		const { getByText } = render(NpcDetail);
		expect(getByText('No relationships defined.')).toBeTruthy();
	});

	it('shows knowledge entries when present', () => {
		editorSelectedNpcId.set(1);
		const { getByText } = render(NpcDetail);
		expect(getByText('Knows the local herb lore')).toBeTruthy();
	});

	it('shows "No knowledge entries." when NPC has none', () => {
		editorSelectedNpcId.set(2);
		const { getByText } = render(NpcDetail);
		expect(getByText('No knowledge entries.')).toBeTruthy();
	});

	it('updates the header when selection changes to a different NPC', async () => {
		editorSelectedNpcId.set(1);
		const { getByRole } = render(NpcDetail);
		// Brigid is the initial selection — her name appears in the h3 header.
		expect(getByRole('heading', { level: 3 }).textContent).toContain(
			'Brigid Ní Fhaoláin',
		);

		editorSelectedNpcId.set(2);
		// Svelte 5 reactive updates flush asynchronously — wait for DOM to settle.
		await waitFor(() => {
			expect(getByRole('heading', { level: 3 }).textContent).toContain(
				'Séamus Ó Briain',
			);
		});
	});
});
