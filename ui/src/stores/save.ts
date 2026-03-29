import { writable } from 'svelte/store';
import type { SaveFileInfo, SaveState } from '$lib/types';

/** Whether the save picker modal is visible. */
export const savePickerVisible = writable<boolean>(false);

/** Discovered save files with branch metadata. */
export const saveFiles = writable<SaveFileInfo[]>([]);

/** Current save state (active file + branch). */
export const currentSaveState = writable<SaveState | null>(null);
