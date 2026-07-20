import { describe, expect, it } from 'vitest';
import type { ParishRenderState, ParishTab } from './types';
import { buildNotebookSectionContent } from './sections';

const state = (activeTab: ParishTab) =>
	({
		activeTab,
		world: {
			location_name: 'The Crossroads',
			location_description: 'Four roads meet beside the old bridge.',
			time_label: 'Afternoon',
			hour: 15,
			minute: 40,
			weather: 'clearing',
			season: 'Spring',
			festival: null,
			paused: false,
			inference_paused: false,
			game_epoch_ms: 0,
			speed_factor: 0,
			name_hints: [],
			day_of_week: 'Monday',
		},
		map: {
			locations: [
				{
					id: '1',
					name: 'The Crossroads',
					lat: 53.63,
					lon: -8.11,
					adjacent: false,
					hops: 0,
				},
				{
					id: '2',
					name: "St. Brigid's Church",
					lat: 53.64,
					lon: -8.1,
					adjacent: true,
					hops: 1,
				},
			],
			edges: [['1', '2']],
			player_location: '1',
			transport_label: 'on foot',
			transport_id: 'walking',
		},
		npcs: [
			{
				npc_id: 4,
				name: 'Roisin Connolly',
				real_name: 'Roisin Connolly',
				occupation: 'shopkeeper',
				mood: 'wary',
				introduced: true,
				mood_emoji: '•',
			},
		],
		selectedNpc: {
			npc_id: 4,
			name: 'Roisin Connolly',
			real_name: 'Roisin Connolly',
			occupation: 'shopkeeper',
			mood: 'wary',
			introduced: true,
			mood_emoji: '•',
		},
		journalEntries: [
			{ source: 'system', content: 'You arrive at the crossroads.' },
			{ source: 'Roisin', content: 'The road is quiet today.' },
		],
	}) satisfies Pick<
		ParishRenderState,
		'activeTab' | 'world' | 'map' | 'npcs' | 'selectedNpc' | 'journalEntries'
	>;

describe('illustrated parish notebook sections', () => {
	it.each([
		['notes', 'Parish Notes'],
		['people', 'Roisin Connolly'],
		['places', 'Places in this Parish'],
		['rumours', 'Rumours'],
		['journal', 'Parish Journal'],
	] as const)('gives the %s tab a distinct in-page section', (tab, title) => {
		const section = buildNotebookSectionContent(state(tab));
		expect(section).toMatchObject({ tab, title });
		expect(section.lines.length).toBeGreaterThan(0);
	});

	it('keeps Places a directory and points geography to the separate Map card', () => {
		const places = buildNotebookSectionContent(state('places'));
		expect(places.lines).toContainEqual({
			label: 'Here',
			text: 'The Crossroads',
		});
		expect(places.lines).toContainEqual({
			label: 'Roads from here',
			text: "St. Brigid's Church",
		});
		expect(
			places.lines.find((line) => line.label === 'Geography')?.text,
		).toMatch(/Map card/);
	});

	it('uses recent conversation lines for the Journal section', () => {
		const journal = buildNotebookSectionContent(state('journal'));
		expect(journal.lines).toEqual([
			{ label: 'system', text: 'You arrive at the crossroads.' },
			{ label: 'Roisin', text: 'The road is quiet today.' },
		]);
	});

	it('keeps the four entries the visible notebook page can render', () => {
		const journal = buildNotebookSectionContent({
			...state('journal'),
			journalEntries: Array.from({ length: 6 }, (_, index) => ({
				source: 'system',
				content: `Entry ${index + 1}`,
			})),
		});
		expect(journal.lines.map((line) => line.text)).toEqual([
			'Entry 3',
			'Entry 4',
			'Entry 5',
			'Entry 6',
		]);
	});
});
