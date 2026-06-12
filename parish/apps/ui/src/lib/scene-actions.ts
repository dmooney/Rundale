/**
 * Maps SceneAction values and NPC sprite clicks onto the existing input paths:
 *   travel_to  → submitInput("go to <name>")  (same path as MapPanel clicks)
 *   talk_to    → focus the InputField with the NPC set as addressed_to
 *   inspect    → append a local system entry to the text log; no backend call
 *
 * Unknown location/NPC ids are silent no-ops so a stale or misconfigured
 * scenes.json never causes an error visible to the player.
 */

import { get } from 'svelte/store';
import type { SceneAction, SceneNpcView, MapData, NpcInfo } from './types';
import { submitInput } from '$lib/ipc';
import {
	mapData,
	npcsHere,
	textLog,
	pushErrorLog,
	formatIpcError,
} from '../stores/game';

// ── Pure helpers (unit-testable without a DOM) ────────────────────────────────

/**
 * Resolves a location id (number, from Rust) to a name using `mapData`.
 *
 * `MapData.locations[].id` is a string (the serialised Rust u32 via
 * `#[serde(rename)]`). The backend sends the id as a string in map data but as
 * a plain number in `SceneState.hotspots[].action.travel_to`. We normalise by
 * comparing as strings on both sides.
 *
 * Returns `null` on a miss (unknown id → no-op).
 */
export function resolveLocationName(
	locationId: number,
	data: MapData | null,
): string | null {
	if (!data) return null;
	const target = data.locations.find((l) => l.id === String(locationId));
	return target?.name ?? null;
}

/**
 * Resolves an NPC id to the display name the mention system expects.
 *
 * The mention mechanism (InputField) keys chips on `npc.name` (the display
 * identity), not on `real_name`. `npcsHere` holds the current location's NPCs
 * from the backend.
 *
 * Returns `null` on a miss (NPC not present at this location → no-op).
 */
export function resolveNpcDisplayName(
	npcId: number,
	npcs: NpcInfo[],
): string | null {
	const npc = npcs.find((n) => {
		// npcsHere does not carry a numeric `id`, only `name` and `real_name`.
		// For talk_to resolution we match by real_name against the npc list.
		// If the NPC is unintroduced the display name is a brief description;
		// either way we return `name` (what the mention chip expects).
		return n.real_name === String(npcId) || n.name === String(npcId);
	});
	return npc?.name ?? null;
}

/**
 * Derives the tooltip text for a sprite.
 *
 * Intentionally ignores `npc.real_name` when `npc.introduced` is false:
 * the frontend must never name an NPC the dialogue system would keep anonymous.
 */
export function spriteTooltip(npc: SceneNpcView): string {
	return npc.display_name;
}

// ── Action handlers ───────────────────────────────────────────────────────────

/**
 * Dispatches a SceneAction from a hotspot click or keyboard activation.
 */
export function handleSceneAction(action: SceneAction): void {
	if ('travel_to' in action) {
		const name = resolveLocationName(action.travel_to, get(mapData));
		if (!name) return; // unknown id — no-op
		submitInput(`go to ${name}`).catch((err) => {
			pushErrorLog(`Could not travel: ${formatIpcError(err)}`);
		});
		return;
	}

	if ('talk_to' in action) {
		const npcs = get(npcsHere);
		const name = resolveNpcDisplayName(action.talk_to, npcs);
		if (!name) return; // NPC not present — no-op
		focusInputWithMention(name);
		return;
	}

	if ('inspect' in action) {
		textLog.update((log) => [
			...log,
			{ source: 'system', content: action.inspect },
		]);
	}
}

/**
 * Handles a click on an NPC sprite: focuses the input addressed to that NPC.
 * Uses display_name so the mention chip shows the same identity as the
 * dialogue system (brief description when unintroduced, real name after).
 */
export function handleNpcSpriteClick(npc: SceneNpcView): void {
	focusInputWithMention(npc.display_name);
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/**
 * Focuses the InputField and inserts an `@npcName` mention chip, reusing the
 * same mechanism as the "Speak To" NPC chip row.
 *
 * The InputField does not expose a programmatic API; we dispatch a custom event
 * that a listener attached in the layout can pick up, or we directly call the
 * insertNpcMention function exported from the field.
 *
 * Approach: dispatch a custom DOM event `parish:mention-npc` with the name as
 * `detail`. The InputField (or a wrapper) can listen for this and call its own
 * `insertNpcMention`. This keeps the coupling loose and the action module
 * DOM-free except for the dispatch.
 */
function focusInputWithMention(npcName: string): void {
	if (typeof document === 'undefined') return;
	document.dispatchEvent(
		new CustomEvent('parish:mention-npc', { detail: { npcName } }),
	);
}
