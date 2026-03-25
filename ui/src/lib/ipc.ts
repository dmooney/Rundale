import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
	WorldSnapshot,
	MapData,
	NpcInfo,
	ThemePalette,
	StreamTokenPayload,
	StreamEndPayload,
	TextLogPayload,
	WorldUpdatePayload,
	LoadingPayload
} from './types';

// ── Commands ─────────────────────────────────────────────────────────────────

export const getWorldSnapshot = () => invoke<WorldSnapshot>('get_world_snapshot');

export const getMap = () => invoke<MapData>('get_map');

export const getNpcsHere = () => invoke<NpcInfo[]>('get_npcs_here');

export const getTheme = () => invoke<ThemePalette>('get_theme');

export const submitInput = (text: string) => invoke<void>('submit_input', { text });

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
