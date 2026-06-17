import { describe, expect, it, vi } from 'vitest';
import type { MapData, NpcInfo, SceneNpcView } from '$lib/types';
import {
	activateSceneHotspot,
	activateSceneNpc,
	appendSceneInspectLog,
	sceneNpcRecipient,
	sceneTravelCommand,
} from './scene-actions';

const mapData: MapData = {
	locations: [
		{
			id: '1',
			name: 'The Crossroads',
			lat: 0,
			lon: 0,
			adjacent: false,
			hops: 0,
		},
		{
			id: '2',
			name: "Darcy's Pub",
			lat: 0.1,
			lon: 0.1,
			adjacent: true,
			hops: 1,
		},
	],
	edges: [['1', '2']],
	player_location: '1',
	transport_label: 'on foot',
	transport_id: 'walking',
};

function sceneNpc(overrides: Partial<SceneNpcView> = {}): SceneNpcView {
	return {
		npc_id: 1,
		slot_id: 'behind-bar',
		display_name: 'an older man behind the bar',
		real_name: null,
		introduced: false,
		mood: 'content',
		mood_emoji: '🙂',
		sprite_url: '/sprite.png',
		x: 50,
		y: 52,
		scale: 1,
		flip: false,
		...overrides,
	};
}

describe('scene actions', () => {
	it('maps travel_to hotspots through mapData names', () => {
		expect(sceneTravelCommand({ travel_to: 2 }, mapData)).toBe(
			"go to Darcy's Pub",
		);
	});

	it('returns null for unknown travel targets', () => {
		expect(sceneTravelCommand({ travel_to: 99 }, mapData)).toBeNull();
	});

	it('submits travel commands for travel hotspots', async () => {
		const submitInput = vi.fn().mockResolvedValue(undefined);
		await activateSceneHotspot(
			{ travel_to: 2 },
			{
				mapData,
				submitInput,
				appendSystemLog: vi.fn(),
				onError: vi.fn(),
			},
		);
		expect(submitInput).toHaveBeenCalledWith("go to Darcy's Pub");
	});

	it('appends local system text for inspect hotspots', async () => {
		const appendSystemLog = vi.fn();
		await activateSceneHotspot(
			{ inspect: 'The counter is dark from years of elbows.' },
			{
				mapData,
				submitInput: vi.fn(),
				appendSystemLog,
				onError: vi.fn(),
			},
		);
		expect(appendSystemLog).toHaveBeenCalledWith(
			'The counter is dark from years of elbows.',
		);
	});

	it('requests input focus for talk_to hotspots', async () => {
		const requestNpcFocus = vi.fn();
		await activateSceneHotspot(
			{ talk_to: 1 },
			{
				mapData,
				submitInput: vi.fn(),
				appendSystemLog: vi.fn(),
				onError: vi.fn(),
				sceneNpcs: [sceneNpc()],
				npcsHere: [
					{
						name: 'an older man behind the bar',
						real_name: 'Padraig Darcy',
						occupation: 'Publican',
						mood: 'content',
						introduced: false,
						mood_emoji: '🙂',
					},
				],
				requestNpcFocus,
			},
		);
		expect(requestNpcFocus).toHaveBeenCalledWith(
			'an older man behind the bar',
			'Padraig Darcy',
		);
	});

	it('resolves unintroduced sprite clicks through npcsHere real names', () => {
		const npcsHere: NpcInfo[] = [
			{
				name: 'an older man behind the bar',
				real_name: 'Padraig Darcy',
				occupation: 'Publican',
				mood: 'content',
				introduced: false,
				mood_emoji: '🙂',
			},
		];
		expect(sceneNpcRecipient(sceneNpc(), npcsHere)).toEqual({
			displayName: 'an older man behind the bar',
			realName: 'Padraig Darcy',
		});
	});

	it('requests input focus for sprite clicks', () => {
		const requestNpcFocus = vi.fn();
		activateSceneNpc(
			sceneNpc({ real_name: 'Padraig Darcy', introduced: true }),
			{
				npcsHere: [],
				requestNpcFocus,
			},
		);
		expect(requestNpcFocus).toHaveBeenCalledWith(
			'an older man behind the bar',
			'Padraig Darcy',
		);
	});

	it('adds inspect log entries without changing existing rows', () => {
		expect(
			appendSceneInspectLog(
				[{ source: 'system', content: 'Before' }],
				'The hearth glows.',
			),
		).toEqual([
			{ source: 'system', content: 'Before' },
			{
				source: 'system',
				subtype: 'scene-inspect',
				content: 'The hearth glows.',
			},
		]);
	});
});
