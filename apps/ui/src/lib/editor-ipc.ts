/**
 * Editor IPC bindings for the Parish Designer.
 *
 * Reuses the transport-agnostic `command` helper from ipc.ts so these
 * work identically in Tauri (desktop) and browser (web server) modes.
 */

import { command } from './ipc';
import type {
	ModSummary,
	EditorModSnapshot,
	ValidationReport,
	EditorSaveResponse,
	EditorDoc,
	NpcFile,
	LocationData
} from './editor-types';

export const editorListMods = () => command<ModSummary[]>('editor_list_mods');

export const editorOpenMod = (modPath: string) =>
	command<EditorModSnapshot>('editor_open_mod', { modPath });

export const editorGetSnapshot = () => command<EditorModSnapshot>('editor_get_snapshot');

export const editorValidate = () => command<ValidationReport>('editor_validate');

export const editorUpdateNpcs = (npcs: NpcFile) =>
	command<ValidationReport>('editor_update_npcs', { npcs });

export const editorUpdateLocations = (locations: LocationData[]) =>
	command<ValidationReport>('editor_update_locations', { locations });

export const editorSave = (docs: EditorDoc[]) =>
	command<EditorSaveResponse>('editor_save', { docs });

export const editorReload = () => command<EditorModSnapshot>('editor_reload');

export const editorClose = () => command<void>('editor_close');
