import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

import {
	loadingColor,
	loadingPhrase,
	mapData,
	npcsHere,
	streamingActive,
	textLog,
	worldState,
} from '../stores/game';
import type { MapData, NpcInfo, ReconnectState, WorldSnapshot } from './types';
import { SceneDeduplicator } from './scene-dedup';
import {
	isReconnectState,
	refreshCanonicalStateAfterWorldUpdate,
	resetPresentationForNewContext,
	resyncCanonicalStateAfterReconnect,
	resyncCanonicalStateAfterSubscription,
	type ReconnectPresentationState,
} from './page-controller';

function snapshot(
	locationDescription: string,
	locationName = 'The Crossroads',
): WorldSnapshot {
	return {
		location_id: 1,
		location_name: locationName,
		location_description: locationDescription,
		time_label: 'Morning',
		hour: 8,
		minute: 0,
		weather: 'Clear',
		season: 'Spring',
		festival: null,
		speed_factor: 36,
		paused: false,
		inference_paused: false,
		game_epoch_ms: 0,
		name_hints: [],
		active_tasks: [],
		day_of_week: 'Saturday',
		turn_in_flight: false,
	};
}

function map(playerLocation: string): MapData {
	return {
		locations: [],
		edges: [],
		player_location: playerLocation,
		transport_label: 'on foot',
		transport_id: 'walking',
	};
}

const oldNpc = {
	name: 'Old neighbour',
	real_name: 'Old neighbour',
	occupation: 'Farmer',
	mood: 'watchful',
	introduced: true,
	mood_emoji: '👀',
} satisfies NpcInfo;

function aggregate(
	world: WorldSnapshot,
	nextMap: MapData,
	npcs: NpcInfo[],
	contextEpoch: number,
): ReconnectState {
	return {
		world,
		map: nextMap,
		npcs,
		context_epoch: contextEpoch,
	};
}

function presentation(
	contextEpoch: number | null,
	resetStream = vi.fn(),
	isActive = () => true,
): ReconnectPresentationState {
	return {
		sceneDedup: new SceneDeduplicator(),
		contextEpoch,
		generation: 0,
		isActive,
		resetStream,
	};
}

beforeEach(() => {
	worldState.set(snapshot('The old branch scene.'));
	mapData.set(map('old-map'));
	npcsHere.set([oldNpc]);
	textLog.set([
		{ source: 'player', content: 'Words belonging to the old branch.' },
	]);
	streamingActive.set(false);
	loadingPhrase.set('');
	loadingColor.set([72, 199, 142]);
});

describe('reconnect canonical context resync', () => {
	it('atomically replaces a stale context after an epoch change', async () => {
		const resetStream = vi.fn();
		const state = presentation(7, resetStream);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
		const fresh = snapshot(
			'A fresh branch opens beside the same gate.',
			'The Crossroads',
		);
		const freshMap = map('1');
		const fetchState = vi
			.fn()
			.mockResolvedValue(aggregate(fresh, freshMap, [], 8));

		await expect(
			resyncCanonicalStateAfterReconnect(state, fetchState),
		).resolves.toBe(true);

		expect(resetStream).toHaveBeenCalledOnce();
		expect(state.contextEpoch).toBe(8);
		expect(state.generation).toBe(1);
		expect(get(worldState)).toBe(fresh);
		expect(get(mapData)).toBe(freshMap);
		expect(get(npcsHere)).toEqual([]);
		expect(get(textLog)).toEqual([
			{
				source: 'system',
				subtype: 'location',
				content: 'A fresh branch opens beside the same gate.',
			},
		]);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(
			false,
		);
	});

	it('preserves a same-epoch transcript while refreshing the aggregate', async () => {
		const resetStream = vi.fn();
		const state = presentation(12, resetStream);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
		const oldLog = get(textLog);
		const fresh = snapshot('Canonical prose for the same location.');
		const freshMap = map('1');
		const freshNpcs = [{ ...oldNpc, mood: 'relieved' }];

		await expect(
			resyncCanonicalStateAfterReconnect(state, () =>
				Promise.resolve(aggregate(fresh, freshMap, freshNpcs, 12)),
			),
		).resolves.toBe(true);

		expect(resetStream).toHaveBeenCalledOnce();
		expect(state.contextEpoch).toBe(12);
		expect(state.generation).toBe(1);
		expect(get(textLog)).toBe(oldLog);
		expect(get(worldState)).toBe(fresh);
		expect(get(mapData)).toBe(freshMap);
		expect(get(npcsHere)).toBe(freshNpcs);
	});

	it('preserves an in-flight presentation when the aggregate rejects', async () => {
		const resetStream = vi.fn();
		const state = presentation(12, resetStream);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
		textLog.set([
			{
				id: 'half-streamed',
				source: 'Old neighbour',
				content: 'I was saying',
				stream_turn_id: 42,
				streaming: true,
				latest_chunk: ' saying',
				stream_chunk_id: 2,
			},
		]);
		streamingActive.set(true);
		const oldWorld = get(worldState);
		const oldMap = get(mapData);
		const oldNpcs = get(npcsHere);
		const oldLog = get(textLog);
		const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});

		await expect(
			resyncCanonicalStateAfterReconnect(state, () =>
				Promise.reject(new Error('aggregate map/NPC capture failed')),
			),
		).resolves.toBe(false);

		expect(resetStream).not.toHaveBeenCalled();
		expect(state.contextEpoch).toBe(12);
		expect(state.generation).toBe(0);
		expect(get(worldState)).toBe(oldWorld);
		expect(get(mapData)).toBe(oldMap);
		expect(get(npcsHere)).toBe(oldNpcs);
		expect(get(textLog)).toBe(oldLog);
		expect(get(streamingActive)).toBe(true);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(
			false,
		);
		expect(warning).toHaveBeenCalledWith(
			'Reconnect resync failed:',
			expect.any(Error),
		);
		warning.mockRestore();
	});

	it('rejects a malformed aggregate before touching any presentation state', async () => {
		const resetStream = vi.fn();
		const state = presentation(null, resetStream);
		const oldWorld = get(worldState);
		const oldMap = get(mapData);
		const oldNpcs = get(npcsHere);
		const oldLog = get(textLog);
		const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
		const malformed = {
			...aggregate(snapshot('Invalid candidate'), map('1'), [], 3),
			npcs: null,
		};

		expect(isReconnectState(malformed)).toBe(false);
		await expect(
			resyncCanonicalStateAfterReconnect(state, () =>
				Promise.resolve(malformed),
			),
		).resolves.toBe(false);

		expect(resetStream).not.toHaveBeenCalled();
		expect(state.contextEpoch).toBeNull();
		expect(state.generation).toBe(0);
		expect(get(worldState)).toBe(oldWorld);
		expect(get(mapData)).toBe(oldMap);
		expect(get(npcsHere)).toBe(oldNpcs);
		expect(get(textLog)).toBe(oldLog);
		expect(warning).toHaveBeenCalledWith(
			'Reconnect resync failed: invalid aggregate payload',
		);
		warning.mockRestore();
	});

	it('cancels stream and spinner presentation at a context-reset boundary', () => {
		const resetStream = vi.fn();
		const state = presentation(27, resetStream);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
		streamingActive.set(true);
		loadingPhrase.set('Listening across the parish...');
		loadingColor.set([200, 100, 50]);

		resetPresentationForNewContext(state);

		expect(resetStream).toHaveBeenCalledOnce();
		expect(get(streamingActive)).toBe(false);
		expect(get(loadingPhrase)).toBe('');
		expect(get(loadingColor)).toEqual([72, 199, 142]);
		expect(get(textLog)).toEqual([]);
		expect(state.contextEpoch).toBeNull();
		expect(state.generation).toBe(1);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
	});

	it('uses the post-reset aggregate world instead of a delayed old world push', async () => {
		const state = presentation(4);
		resetPresentationForNewContext(state);
		const canonicalWorld = snapshot(
			'Only the replacement branch prose is authoritative.',
			'Replacement Parish',
		);
		canonicalWorld.location_id = 91;
		const canonicalMap = map('91');
		const canonicalNpcs = [{ ...oldNpc, real_name: 'New neighbour' }];
		let currentRevision = 2;

		// The caller received an old pushed snapshot here, but deliberately
		// refreshes from the aggregate rather than committing that payload.
		await expect(
			refreshCanonicalStateAfterWorldUpdate(
				state,
				2,
				() => currentRevision,
				() =>
					Promise.resolve(
						aggregate(canonicalWorld, canonicalMap, canonicalNpcs, 5),
					),
			),
		).resolves.toBe(canonicalWorld);

		expect(get(worldState)).toBe(canonicalWorld);
		expect(get(mapData)).toBe(canonicalMap);
		expect(get(npcsHere)).toBe(canonicalNpcs);
		expect(get(textLog)).toEqual([
			{
				source: 'system',
				subtype: 'location',
				content: 'Only the replacement branch prose is authoritative.',
			},
		]);
		expect(state.contextEpoch).toBe(5);
		expect(state.generation).toBe(2);
		currentRevision += 1;
	});

	it('rejects an old reconnect response that resolves after a context reset', async () => {
		const state = presentation(10);
		const staleWorld = snapshot('Stale branch prose.', 'Stale Parish');
		const staleMap = map('10');
		let resolveStale!: (value: ReconnectState) => void;
		const staleResponse = new Promise<ReconnectState>((resolve) => {
			resolveStale = resolve;
		});
		const reconnect = resyncCanonicalStateAfterReconnect(
			state,
			() => staleResponse,
		);

		resetPresentationForNewContext(state);
		const replacementWorld = snapshot(
			'Replacement branch prose.',
			'Replacement Parish',
		);
		replacementWorld.location_id = 11;
		const replacementMap = map('11');
		const replacementNpcs = [{ ...oldNpc, real_name: 'Replacement neighbour' }];
		await refreshCanonicalStateAfterWorldUpdate(
			state,
			1,
			() => 1,
			() =>
				Promise.resolve(
					aggregate(replacementWorld, replacementMap, replacementNpcs, 11),
				),
		);

		resolveStale(aggregate(staleWorld, staleMap, [oldNpc], 10));
		await expect(reconnect).resolves.toBe(false);

		expect(get(worldState)).toBe(replacementWorld);
		expect(get(mapData)).toBe(replacementMap);
		expect(get(npcsHere)).toBe(replacementNpcs);
		expect(state.contextEpoch).toBe(11);
	});

	it('post-subscription resync replaces an initial aggregate overtaken before listeners attached', async () => {
		const resetStream = vi.fn();
		const state = presentation(30, resetStream);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
		const replacementWorld = snapshot(
			'The post-subscription branch is authoritative.',
			'The Crossroads',
		);
		replacementWorld.location_id = 31;
		const replacementMap = map('31');
		const replacementNpcs = [{ ...oldNpc, real_name: 'Current neighbour' }];

		await expect(
			resyncCanonicalStateAfterSubscription(state, () =>
				Promise.resolve(
					aggregate(replacementWorld, replacementMap, replacementNpcs, 31),
				),
			),
		).resolves.toBe(true);

		expect(resetStream).toHaveBeenCalledOnce();
		expect(get(worldState)).toBe(replacementWorld);
		expect(get(mapData)).toBe(replacementMap);
		expect(get(npcsHere)).toBe(replacementNpcs);
		expect(get(textLog)).toEqual([
			{
				source: 'system',
				subtype: 'location',
				content: 'The post-subscription branch is authoritative.',
			},
		]);
		expect(state.contextEpoch).toBe(31);
	});

	it('post-subscription same-epoch reconciliation preserves a stream begun during setup', async () => {
		const resetStream = vi.fn();
		const state = presentation(8, resetStream);
		expect(state.sceneDedup.shouldShowDescription('The Crossroads')).toBe(true);
		textLog.set([
			{
				id: 'setup-stream',
				source: oldNpc.real_name,
				content: 'A stream that began while subscribing',
				stream_turn_id: 8,
				streaming: true,
			},
		]);
		streamingActive.set(true);
		const oldLog = get(textLog);
		const currentWorld = snapshot('Same-context canonical prose.');
		currentWorld.turn_in_flight = true;

		await expect(
			resyncCanonicalStateAfterSubscription(state, () =>
				Promise.resolve(aggregate(currentWorld, map('1'), [oldNpc], 8)),
			),
		).resolves.toBe(true);

		expect(resetStream).not.toHaveBeenCalled();
		expect(get(textLog)).toBe(oldLog);
		expect(get(streamingActive)).toBe(true);
		expect(get(worldState)).toBe(currentWorld);
	});

	it('does not mutate stores when cleanup disposes an in-flight reconnect', async () => {
		let active = true;
		const resetStream = vi.fn();
		const state = presentation(14, resetStream, () => active);
		let resolveReconnect!: (value: ReconnectState) => void;
		const response = new Promise<ReconnectState>((resolve) => {
			resolveReconnect = resolve;
		});
		const oldWorld = get(worldState);
		const oldMap = get(mapData);
		const oldNpcs = get(npcsHere);
		const oldLog = get(textLog);
		const reconnect = resyncCanonicalStateAfterReconnect(state, () => response);

		// Mirrors controller cleanup: mark inactive and advance the generation
		// before listeners/stream manager are disposed.
		active = false;
		state.generation += 1;
		resolveReconnect(
			aggregate(snapshot('Must never render.'), map('99'), [], 15),
		);

		await expect(reconnect).resolves.toBe(false);
		expect(resetStream).not.toHaveBeenCalled();
		expect(get(worldState)).toBe(oldWorld);
		expect(get(mapData)).toBe(oldMap);
		expect(get(npcsHere)).toBe(oldNpcs);
		expect(get(textLog)).toBe(oldLog);
	});
});
