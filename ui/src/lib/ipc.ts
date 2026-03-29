import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
	WorldSnapshot,
	MapData,
	NpcInfo,
	ThemePalette,
	UiConfig,
	StreamTokenPayload,
	StreamEndPayload,
	TextLogPayload,
	WorldUpdatePayload,
	LoadingPayload,
	DebugSnapshot,
	SaveFileInfo,
	SaveState
} from './types';

// ── Commands ─────────────────────────────────────────────────────────────────

export const getWorldSnapshot = () => invoke<WorldSnapshot>('get_world_snapshot');

export const getMap = () => invoke<MapData>('get_map');

export const getNpcsHere = () => invoke<NpcInfo[]>('get_npcs_here');

export const getTheme = () => invoke<ThemePalette>('get_theme');

export const submitInput = (text: string) => invoke<void>('submit_input', { text });

export const getDebugSnapshot = () => invoke<DebugSnapshot>('get_debug_snapshot');

export const getUiConfig = () => invoke<UiConfig>('get_ui_config');

// ── Persistence commands ────────────────────────────────────────────────────

export const discoverSaveFiles = () => invoke<SaveFileInfo[]>('discover_save_files');

export const saveGame = () => invoke<string>('save_game');

export const loadBranch = (filePath: string, branchId: number) =>
	invoke<void>('load_branch', { filePath, branchId });

export const createBranch = (name: string, parentBranchId: number) =>
	invoke<string>('create_branch', { name, parentBranchId });

export const newSaveFile = () => invoke<void>('new_save_file');

export const newGame = () => invoke<void>('new_game');

export const getSaveState = () => invoke<SaveState>('get_save_state');

// ── Events ───────────────────────────────────────────────────────────────────

export const onStreamToken = (cb: (payload: StreamTokenPayload) => void) =>
	listen<StreamTokenPayload>('stream-token', (e) => cb(e.payload));

export const onStreamEnd = (cb: (payload: StreamEndPayload) => void) =>
	listen<StreamEndPayload>('stream-end', (e) => cb(e.payload));

export const onTextLog = (cb: (payload: TextLogPayload) => void) =>
	listen<TextLogPayload>('text-log', (e) => cb(e.payload));

export const onWorldUpdate = (cb: (payload: WorldUpdatePayload) => void) =>
	listen<WorldUpdatePayload>('world-update', (e) => cb(e.payload));

export const onLoading = (cb: (payload: LoadingPayload) => void) =>
	listen<LoadingPayload>('loading', (e) => cb(e.payload));

export const onThemeUpdate = (cb: (payload: ThemePalette) => void) =>
	listen<ThemePalette>('theme-update', (e) => cb(e.payload));

export const onDebugUpdate = (cb: (payload: DebugSnapshot) => void) =>
	listen<DebugSnapshot>('debug-update', (e) => cb(e.payload));

export const onSavePicker = (cb: () => void) =>
	listen<void>('save-picker', () => cb());
