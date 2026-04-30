/**
 * Svelte stores for the Parish Designer editor.
 *
 * Pattern follows apps/ui/src/stores/debug.ts — simple writable stores
 * updated from IPC call results.
 */

import { writable, derived } from 'svelte/store';
import type {
	ModSummary,
	EditorModSnapshot,
	ValidationReport,
	EditorTab,
	NpcFileEntry,
	LocationData
} from '$lib/editor-types';

/** Available mods discovered on disk. */
export const editorMods = writable<ModSummary[]>([]);

/** The currently loaded mod snapshot (null if no mod is open). */
export const editorSnapshot = writable<EditorModSnapshot | null>(null);

/** Active tab in the editor shell. */
export const editorTab = writable<EditorTab>('mods');

/** Selected NPC id in the NPC list (null if none). */
export const editorSelectedNpcId = writable<number | null>(null);

/** Selected location id in the location list (null if none). */
export const editorSelectedLocationId = writable<number | null>(null);

/** Whether the in-memory snapshot differs from the on-disk version. */
export const editorDirty = writable<boolean>(false);

/** The most recent validation report. */
export const editorValidation = writable<ValidationReport | null>(null);

// ── Derived stores ─────────────────────────────────────────────────────────

/** The NPC list from the current snapshot. */
export const editorNpcs = derived(editorSnapshot, ($snap) => $snap?.npcs.npcs ?? []);

/** The location list from the current snapshot. */
export const editorLocations = derived(editorSnapshot, ($snap) => $snap?.locations ?? []);

/** The currently selected NPC entry. */
export const editorSelectedNpc = derived(
	[editorNpcs, editorSelectedNpcId],
	([$npcs, $id]) => ($id !== null ? $npcs.find((n) => n.id === $id) ?? null : null)
);

/** The currently selected location entry. */
export const editorSelectedLocation = derived(
	[editorLocations, editorSelectedLocationId],
	([$locs, $id]) => ($id !== null ? $locs.find((l) => l.id === $id) ?? null : null)
);

/** Total error + warning count for the badge. */
export const editorIssueCount = derived(editorValidation, ($v) =>
	$v ? $v.errors.length + $v.warnings.length : 0
);
