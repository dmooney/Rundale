import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { NpcInfo, WorldSnapshot } from '$lib/types';
import type { NotebookViewModelInput } from './types';
import {
	buildNotebookViewModel,
	MAX_NOTEBOOK_LIVE_LINES,
	notebookNpcLabel,
} from './view-model';

const world: WorldSnapshot = {
	location_name: 'Kilteevan Village',
	location_description: 'A whitewashed village beside the bridge.',
	time_label: 'Afternoon',
	hour: 15,
	minute: 40,
	weather: 'Clearing',
	season: 'Spring',
	festival: null,
	paused: false,
	inference_paused: false,
	game_epoch_ms: Date.UTC(1820, 3, 1, 15, 40),
	speed_factor: 36,
	name_hints: [],
	day_of_week: 'Monday',
};

const roisin: NpcInfo = {
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};

function build(
	overrides: Partial<NotebookViewModelInput> = {},
): ReturnType<typeof buildNotebookViewModel> {
	return buildNotebookViewModel({
		world,
		npcs: [roisin],
		selectedNpc: roisin,
		textLog: [],
		busy: false,
		intentText: '',
		...overrides,
	});
}

describe('illustrated notebook view model', () => {
	it('turns live player, location, action, and NPC entries into visible lines', () => {
		const view = build({
			textLog: [
				{ id: 'p1', source: 'player', content: 'examine the potato patch' },
				{
					id: 'l1',
					source: 'system',
					subtype: 'location',
					content: 'You arrive at Murphy’s Farm.',
				},
				{
					id: 'a1',
					source: 'system',
					content: 'You kneel and break the first clod of soil.',
				},
				{
					id: 'n1',
					source: 'Roisin Connolly',
					content: 'Mind the shallow row by the wall.',
					streaming: true,
				},
			],
			busy: true,
		});

		expect(view.liveLines).toMatchObject([
			{
				kind: 'player',
				speaker: 'You',
				content: 'examine the potato patch',
			},
			{
				kind: 'location',
				speaker: 'Place',
				content: 'You arrive at Murphy’s Farm.',
			},
			{
				kind: 'narration',
				speaker: 'Parish',
				content: 'You kneel and break the first clod of soil.',
			},
			{
				kind: 'npc',
				speaker: 'Roisin Connolly',
				content: 'Mind the shallow row by the wall.',
				streaming: true,
			},
		]);
		expect(view.liveTitle).toContain('listening');
		expect(view.person?.recentLines).toHaveLength(1);
	});

	it('derives person, place, draft, and prompt copy from the current context', () => {
		const view = build({
			world: {
				...world,
				location_name: 'The Crossroads',
				location_description: 'Four narrow roads meet by a bramble wall.',
			},
			intentText: 'ask about the old road',
			textLog: [
				{
					source: 'Roisin Connolly',
					content: 'I have not travelled that road today.',
				},
			],
		});

		expect(view.locationName).toBe('The Crossroads');
		expect(view.locationDescription).toContain('Four narrow roads');
		expect(view.person).toMatchObject({
			label: 'Roisin Connolly',
			mood: 'wary',
			detail: 'shopkeeper',
		});
		expect(view.person?.recentLines[0].content).toContain('not travelled');
		expect(view.intentPlaceholder).toBe(
			'Write what you say to Roisin Connolly…',
		);
		expect(view.draftSummary).toBe('ask about the old road');
	});

	it('uses honest empty states when no live context has arrived', () => {
		const view = build({
			world: null,
			npcs: [],
			selectedNpc: null,
			textLog: [],
		});

		expect(view.locationName).toBe('Location not yet known');
		expect(view.locationDescription).toContain('No description');
		expect(view.person).toBeNull();
		expect(view.liveLines).toEqual([]);
		expect(view.liveEmpty).toContain('will appear here');
		expect(view.intentPlaceholder).toBe('Write what you do next…');
	});

	it('never exposes canonical names or occupations for unintroduced people', () => {
		const stranger: NpcInfo = {
			name: 'a lean, red-haired stranger with hard eyes',
			real_name: 'Sean Ruadh Kelly',
			occupation: 'labourer',
			mood: 'guarded',
			introduced: false,
			mood_emoji: '•',
		};
		const view = build({
			npcs: [stranger],
			selectedNpc: stranger,
			textLog: [
				{
					source: 'Sean Ruadh Kelly',
					content: 'Sean Ruadh Kelly says the gate needs mending.',
				},
			],
		});

		expect(notebookNpcLabel(stranger)).toBe('Lean, red-haired stranger');
		expect(view.person?.detail).toBe('Not yet introduced');
		expect(view.liveLines[0].speaker).toBe('Lean, red-haired stranger');
		expect(view.liveLines[0].content).toBe(
			'Lean, red-haired stranger says the gate needs mending.',
		);
		expect(JSON.stringify(view)).not.toContain('Sean Ruadh Kelly');

		const leakedDisplay = {
			...stranger,
			name: 'Sean Ruadh Kelly',
		};
		expect(notebookNpcLabel(leakedDisplay)).toBe('Unintroduced person');
	});

	it('keeps the current player turn visible while bounding a noisy log', () => {
		const textLog = [
			{ source: 'system', content: 'An old line.' },
			{ source: 'player', content: 'latest command' },
			...Array.from({ length: 7 }, (_, index) => ({
				source: 'system',
				content: `result ${index + 1}`,
			})),
		];

		const view = build({ textLog });

		expect(view.liveLines).toHaveLength(MAX_NOTEBOOK_LIVE_LINES);
		expect(view.liveLines[0]).toMatchObject({
			kind: 'player',
			content: 'latest command',
		});
		expect(view.liveLines.at(-1)?.content).toBe('result 7');
	});

	it('does not ship the concept-only character facts or named prompt', () => {
		const productionSource = [
			readFileSync(
				resolve(process.cwd(), 'src/lib/illustrated-notebook/renderer.ts'),
				'utf8',
			),
			readFileSync(
				resolve(
					process.cwd(),
					'src/components/illustrated-notebook/IllustratedNotebookGame.svelte',
				),
				'utf8',
			),
		].join('\n');

		for (const shippedLiteral of [
			'ask Roisin what she saw',
			'cart delayed',
			'flour is short',
			'saw who crossed the bridge',
			'watching the road',
			'She knows',
		]) {
			expect(productionSource).not.toContain(shippedLiteral);
		}
	});
});
