import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import type { NpcInfo, WorldSnapshot } from '$lib/types';
import type { NotebookViewModelInput } from './types';
import {
	buildNotebookViewModel,
	MAX_NOTEBOOK_LIVE_LINES,
	notebookNpcLabel,
	notebookReactionSummary,
	selectVisibleNotebookLines,
} from './view-model';

const world: WorldSnapshot = {
	location_id: 15,
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
	active_tasks: [],
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

	it('keeps slash commands distinct from spoken player dialogue', () => {
		const view = build({
			textLog: [
				{
					id: 'command',
					source: 'player',
					subtype: 'command',
					content: '/pause',
				},
				{ id: 'speech', source: 'player', content: 'Good morning.' },
			],
		});

		expect(view.liveLines).toMatchObject([
			{ kind: 'command', speaker: 'Command', content: '/pause' },
			{ kind: 'player', speaker: 'You', content: 'Good morning.' },
		]);
	});

	it('renders the production action source as parish narration', () => {
		const view = build({
			textLog: [
				{
					id: 'action',
					source: 'action',
					content: 'You turn over the first row of soil.',
				},
			],
		});

		expect(view.liveLines).toMatchObject([
			{
				kind: 'narration',
				speaker: 'Parish',
				content: 'You turn over the first row of soil.',
			},
		]);
	});

	it('renders an NPC-sourced action subtype as narration, never dialogue', () => {
		const view = build({
			textLog: [
				{
					id: 'gesture',
					source: 'Roisin Connolly',
					subtype: 'action',
					content: 'She points toward the shallow row by the wall.',
				},
			],
		});

		expect(view.liveLines).toMatchObject([
			{
				kind: 'narration',
				speaker: 'Parish',
				content: 'She points toward the shallow row by the wall.',
			},
		]);
		expect(view.person?.recentLines).toEqual([]);
	});

	it('derives person, place, and prompt copy from the current context', () => {
		const view = build({
			world: {
				...world,
				location_name: 'The Crossroads',
				location_description: 'Four narrow roads meet by a bramble wall.',
			},
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
	});

	it('derives active progression only from canonical tasks and prefers work in progress', () => {
		const view = build({
			world: {
				...world,
				active_tasks: [
					{
						id: 10,
						description: 'Carry seed potatoes to the lower field',
						assigned_by: 4,
						location_id: 9,
						status: 'assigned',
						assigned_at: '1820-04-01T15:30:00Z',
						started_at: null,
						completed_at: null,
						last_matching_action: null,
					},
					{
						id: 11,
						description: 'Break the first row in the potato patch',
						assigned_by: 4,
						location_id: 9,
						status: 'in_progress',
						assigned_at: '1820-04-01T15:35:00Z',
						started_at: '1820-04-01T15:40:00Z',
						completed_at: null,
						last_matching_action: 'I set to work in the potato patch.',
					},
					{
						id: 12,
						description: 'A task already finished',
						assigned_by: 4,
						location_id: 9,
						status: 'completed',
						assigned_at: '1820-04-01T14:00:00Z',
						started_at: '1820-04-01T14:05:00Z',
						completed_at: '1820-04-01T14:30:00Z',
						last_matching_action: 'I finished it.',
					},
				],
			},
		});

		expect(view.currentTask).toMatchObject({
			id: 11,
			description: 'Break the first row in the potato patch',
			status: 'in_progress',
			statusLabel: 'In progress',
		});
		expect(view.activeTasks.map((task) => task.id)).toEqual([11, 10]);
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
		expect(view.currentTask).toBeNull();
		expect(view.activeTasks).toEqual([]);
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
					reactions: [{ emoji: '👀', source: 'Sean Ruadh Kelly' }],
				},
			],
		});

		expect(notebookNpcLabel(stranger)).toBe('Lean, red-haired stranger');
		expect(view.person?.detail).toBe('Not yet introduced');
		expect(view.liveLines[0].speaker).toBe('Lean, red-haired stranger');
		expect(view.liveLines[0].content).toBe(
			'Lean, red-haired stranger says the gate needs mending.',
		);
		expect(view.liveLines[0].reactions).toEqual([
			{ emoji: '👀', source: 'Lean, red-haired stranger' },
		]);
		expect(notebookReactionSummary(view.liveLines[0].reactions)).toBe(
			'👀 Lean, red-haired stranger',
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

	it.each([
		['desktop', 4],
		['roomy mobile', 3],
		['compact landscape', 2],
	])(
		'keeps the latest command visible inside the %s Pixi line budget',
		(_mode, limit) => {
			const lines = [
				{
					key: 'command',
					kind: 'command' as const,
					speaker: 'Command',
					content: '/status',
					streaming: false,
					messageId: null,
					reactions: [],
				},
				...Array.from({ length: 5 }, (_, index) => ({
					key: `response-${index}`,
					kind: 'narration' as const,
					speaker: 'Parish',
					content: `response ${index + 1}`,
					streaming: false,
					messageId: null,
					reactions: [],
				})),
			];

			const visible = selectVisibleNotebookLines(lines, limit);

			expect(visible).toHaveLength(limit);
			expect(visible[0]).toMatchObject({
				key: 'command',
				kind: 'command',
				content: '/status',
			});
			if (limit > 1) {
				expect(visible.at(-1)?.content).toBe('response 5');
			}
		},
	);

	it.each([
		['system narration', 'narration'],
		['action narration', 'narration'],
		['location', 'location'],
		['NPC dialogue', 'npc'],
	] as const)(
		'compact current-turn pair replaces old output with newest %s',
		(_label, newestKind) => {
			const visible = selectVisibleNotebookLines(
				[
					{
						key: 'player',
						kind: 'player',
						speaker: 'You',
						content: 'look toward the road',
						streaming: false,
						messageId: null,
						reactions: [],
					},
					{
						key: 'old-output',
						kind: 'narration',
						speaker: 'Parish',
						content: 'An older result.',
						streaming: false,
						messageId: null,
						reactions: [],
					},
					{
						key: 'newest-output',
						kind: newestKind,
						speaker: newestKind === 'npc' ? 'Séamas' : 'Parish',
						content: 'The newest authoritative result.',
						streaming: false,
						messageId: null,
						reactions: [],
					},
				],
				2,
			);

			expect(visible.map((line) => line.key)).toEqual([
				'player',
				'newest-output',
			]);
		},
	);

	it('gives an active streamed NPC line highest compact output priority', () => {
		const visible = selectVisibleNotebookLines(
			[
				{
					key: 'player',
					kind: 'command',
					speaker: 'Command',
					content: 'ask Séamas about the road',
					streaming: false,
					messageId: null,
					reactions: [],
				},
				{
					key: 'active-stream',
					kind: 'npc',
					speaker: 'Séamas',
					content: 'I saw a cart',
					streaming: true,
					messageId: 'active-stream',
					reactions: [],
				},
				{
					key: 'later-status',
					kind: 'narration',
					speaker: 'Parish',
					content: 'The clock advances.',
					streaming: false,
					messageId: null,
					reactions: [],
				},
			],
			2,
		);

		expect(visible.map((line) => line.key)).toEqual([
			'player',
			'active-stream',
		]);
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
