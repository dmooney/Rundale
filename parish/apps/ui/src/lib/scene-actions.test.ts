import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import {
	resolveLocationName,
	resolveNpcDisplayName,
	spriteTooltip,
	handleSceneAction,
	handleNpcSpriteClick,
} from './scene-actions';
import type { MapData, NpcInfo, SceneNpcView } from './types';
import { mapData, npcsHere, textLog } from '../stores/game';

// ── IPC mock ────────────────────────────────────────────────────────────────
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const mockSubmitInput = vi.fn(
	(_text: string, _addressedTo?: string[]): Promise<void> => Promise.resolve(),
);

vi.mock('$lib/ipc', () => ({
	submitInput: (text: string, addressedTo?: string[]) =>
		mockSubmitInput(text, addressedTo),
}));

// ── Helpers ─────────────────────────────────────────────────────────────────

const MAP: MapData = {
	locations: [
		{
			id: '1',
			name: 'The Crossroads',
			lat: 0,
			lon: 0,
			adjacent: true,
			hops: 1,
		},
		{ id: '2', name: "Darcy's Pub", lat: 0, lon: 0, adjacent: true, hops: 1 },
	],
	edges: [['1', '2']],
	player_location: '1',
	transport_label: 'on foot',
	transport_id: 'walking',
};

const NPCS: NpcInfo[] = [
	{
		name: 'Padraig Darcy',
		real_name: 'Padraig Darcy',
		occupation: 'Publican',
		mood: 'cheerful',
		introduced: true,
		mood_emoji: '😊',
	},
	{
		name: 'an older woman',
		real_name: 'Brigid Walsh',
		occupation: 'Farmer',
		mood: 'neutral',
		introduced: false,
		mood_emoji: '😐',
	},
];

const INTRODUCED_NPC: SceneNpcView = {
	id: 1,
	display_name: 'Padraig Darcy',
	real_name: 'Padraig Darcy',
	introduced: true,
	mood_emoji: '😊',
	sprite_url: 'data:image/png;base64,abc',
	x: 48,
	y: 55,
	scale: 1.0,
	flip: false,
};

const UNINTRODUCED_NPC: SceneNpcView = {
	id: 2,
	display_name: 'an older woman',
	real_name: 'Brigid Walsh',
	introduced: false,
	mood_emoji: '😐',
	sprite_url: 'data:image/png;base64,def',
	x: 22,
	y: 68,
	scale: 1.1,
	flip: true,
};

// ── resolveLocationName ──────────────────────────────────────────────────────

describe('resolveLocationName', () => {
	it('returns the name for a known numeric id', () => {
		expect(resolveLocationName(1, MAP)).toBe('The Crossroads');
		expect(resolveLocationName(2, MAP)).toBe("Darcy's Pub");
	});

	it('returns null for an unknown id', () => {
		expect(resolveLocationName(99, MAP)).toBeNull();
	});

	it('returns null when mapData is null', () => {
		expect(resolveLocationName(1, null)).toBeNull();
	});
});

// ── resolveNpcDisplayName ────────────────────────────────────────────────────

describe('resolveNpcDisplayName', () => {
	it('returns the npc name when real_name matches', () => {
		expect(resolveNpcDisplayName(0, NPCS)).toBeNull(); // no match
	});

	it('resolves by real_name string match', () => {
		// For introduced NPCs real_name and name are the same string
		const result = resolveNpcDisplayName(0, [
			{ ...NPCS[0], real_name: '0' } as NpcInfo,
		]);
		expect(result).toBe('Padraig Darcy');
	});

	it('returns null for an NPC not in the list', () => {
		expect(resolveNpcDisplayName(999, NPCS)).toBeNull();
	});
});

// ── spriteTooltip ────────────────────────────────────────────────────────────

describe('spriteTooltip', () => {
	it('returns display_name for an introduced NPC', () => {
		expect(spriteTooltip(INTRODUCED_NPC)).toBe('Padraig Darcy');
	});

	it('returns display_name (brief description) for an unintroduced NPC', () => {
		expect(spriteTooltip(UNINTRODUCED_NPC)).toBe('an older woman');
	});

	it('never returns real_name for an unintroduced NPC', () => {
		const tooltip = spriteTooltip(UNINTRODUCED_NPC);
		expect(tooltip).not.toBe('Brigid Walsh');
	});
});

// ── handleSceneAction: travel_to ─────────────────────────────────────────────

describe('handleSceneAction — travel_to', () => {
	beforeEach(() => {
		mockSubmitInput.mockClear();
		mapData.set(MAP);
	});

	it('calls submitInput with the resolved location name', async () => {
		handleSceneAction({ travel_to: 2 });
		await Promise.resolve(); // let the microtask queue drain
		expect(mockSubmitInput).toHaveBeenCalled();
		expect(mockSubmitInput.mock.calls[0][0]).toBe("go to Darcy's Pub");
	});

	it('is a no-op for an unknown location id', () => {
		handleSceneAction({ travel_to: 999 });
		expect(mockSubmitInput).not.toHaveBeenCalled();
	});

	it('is a no-op when mapData is null', () => {
		mapData.set(null);
		handleSceneAction({ travel_to: 1 });
		expect(mockSubmitInput).not.toHaveBeenCalled();
	});
});

// ── handleSceneAction: inspect ───────────────────────────────────────────────

describe('handleSceneAction — inspect', () => {
	beforeEach(() => {
		textLog.set([]);
	});

	it('appends a local system entry to the text log', () => {
		handleSceneAction({ inspect: 'A turf fire smoulders.' });
		const log = get(textLog);
		expect(log).toHaveLength(1);
		expect(log[0]).toMatchObject({
			source: 'system',
			content: 'A turf fire smoulders.',
		});
	});

	it('does not call submitInput', () => {
		mockSubmitInput.mockClear();
		handleSceneAction({ inspect: 'Some text.' });
		expect(mockSubmitInput).not.toHaveBeenCalled();
	});
});

// ── handleSceneAction: talk_to ────────────────────────────────────────────────

describe('handleSceneAction — talk_to', () => {
	const dispatchedEvents: CustomEvent[] = [];

	beforeEach(() => {
		dispatchedEvents.length = 0;
		npcsHere.set(NPCS);
		function capture(e: Event) {
			dispatchedEvents.push(e as CustomEvent);
		}
		document.addEventListener('parish:mention-npc', capture);
		// Store a reference so we can remove it after
		(globalThis as Record<string, unknown>).__captureRef = capture;
	});

	afterEach(() => {
		const capture = (globalThis as Record<string, unknown>)
			.__captureRef as EventListener;
		if (capture) document.removeEventListener('parish:mention-npc', capture);
	});

	it('dispatches parish:mention-npc for a present NPC matched by real_name', () => {
		// Seed a NPC whose real_name is the string '42'
		npcsHere.set([{ ...NPCS[0], real_name: '42' }]);
		handleSceneAction({ talk_to: 42 });
		expect(dispatchedEvents).toHaveLength(1);
		expect(dispatchedEvents[0].detail).toEqual({ npcName: 'Padraig Darcy' });
	});

	it('is a no-op for an NPC not present at this location', () => {
		handleSceneAction({ talk_to: 999 });
		expect(dispatchedEvents).toHaveLength(0);
	});
});

// ── handleNpcSpriteClick ─────────────────────────────────────────────────────

describe('handleNpcSpriteClick', () => {
	const dispatchedEvents: CustomEvent[] = [];

	beforeEach(() => {
		dispatchedEvents.length = 0;
		function capture(e: Event) {
			dispatchedEvents.push(e as CustomEvent);
		}
		document.addEventListener('parish:mention-npc', capture);
		(globalThis as Record<string, unknown>).__captureRef2 = capture;
	});

	afterEach(() => {
		const capture = (globalThis as Record<string, unknown>)
			.__captureRef2 as EventListener;
		if (capture) document.removeEventListener('parish:mention-npc', capture);
	});

	it('dispatches parish:mention-npc with display_name for an introduced NPC', () => {
		handleNpcSpriteClick(INTRODUCED_NPC);
		expect(dispatchedEvents).toHaveLength(1);
		expect(dispatchedEvents[0].detail).toEqual({ npcName: 'Padraig Darcy' });
	});

	it('dispatches parish:mention-npc with brief description for an unintroduced NPC', () => {
		handleNpcSpriteClick(UNINTRODUCED_NPC);
		expect(dispatchedEvents).toHaveLength(1);
		expect(dispatchedEvents[0].detail).toEqual({ npcName: 'an older woman' });
	});

	it('never dispatches real_name for an unintroduced NPC', () => {
		handleNpcSpriteClick(UNINTRODUCED_NPC);
		expect(dispatchedEvents[0].detail.npcName).not.toBe('Brigid Walsh');
	});
});
