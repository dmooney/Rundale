import { writable } from 'svelte/store';
import type { DebugSnapshot } from '$lib/types';

/** Whether the debug panel is visible. */
export const debugVisible = writable<boolean>(false);

/** Latest debug snapshot from the backend. */
export const debugSnapshot = writable<DebugSnapshot | null>(null);

/** Active debug tab index (0=Overview, 1=NPCs, 2=World, 3=Weather, 4=Gossip, 5=Conv, 6=Events, 7=Inference). */
export const debugTab = writable<number>(0);

/** Preferred dock position for debug panel on wide screens. */
export const debugDockLeft = writable<boolean>(false);

/** ID of the selected NPC for deep-dive, or null for list view. */
export const selectedNpcId = writable<number | null>(null);
