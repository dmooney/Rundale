/**
 * IPC transport layer — works in both Tauri (desktop) and browser (web server).
 *
 * In Tauri mode, uses `@tauri-apps/api` invoke/listen.
 * In browser mode, uses fetch for commands and WebSocket for events.
 * All exported function signatures are identical regardless of transport.
 */

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

// ── Transport detection ─────────────────────────────────────────────────────

const IS_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

// ── Commands ────────────────────────────────────────────────────────────────

async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
	if (IS_TAURI) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<T>(name, args);
	}
	// Web mode: REST API
	const endpoint = `/api/${name.replace(/^get_/, '').replace(/_/g, '-')}`;
	const resp = await fetch(endpoint, {
		method: args ? 'POST' : 'GET',
		headers: args ? { 'Content-Type': 'application/json' } : {},
		body: args ? JSON.stringify(args) : undefined
	});
	if (!resp.ok) {
		throw new Error(`API error: ${resp.status} ${resp.statusText}`);
	}
	// submit_input returns 200 with no body
	const text = await resp.text();
	if (!text) return undefined as T;
	return JSON.parse(text) as T;
}

export const getWorldSnapshot = () => command<WorldSnapshot>('get_world_snapshot');

export const getMap = () => command<MapData>('get_map');

export const getNpcsHere = () => command<NpcInfo[]>('get_npcs_here');

export const getTheme = () => command<ThemePalette>('get_theme');

export const submitInput = (text: string) => command<void>('submit_input', { text });

export const getDebugSnapshot = () => command<DebugSnapshot>('get_debug_snapshot');

export const getUiConfig = () => command<UiConfig>('get_ui_config');

// ── Persistence commands ────────────────────────────────────────────────────

export const discoverSaveFiles = () => command<SaveFileInfo[]>('discover_save_files');

export const saveGame = () => command<string>('save_game');

export const loadBranch = (filePath: string, branchId: number) =>
	command<void>('load_branch', { filePath, branchId });

export const createBranch = (name: string, parentBranchId: number) =>
	command<string>('create_branch', { name, parentBranchId });

export const newSaveFile = () => command<void>('new_save_file');

export const newGame = () => command<void>('new_game');

export const getSaveState = () => command<SaveState>('get_save_state');

// ── Events ──────────────────────────────────────────────────────────────────

type UnlistenFn = () => void;
type EventCallback<T> = (payload: T) => void;

// WebSocket state for browser mode
let ws: WebSocket | null = null;
let wsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
const wsListeners = new Map<string, Set<EventCallback<unknown>>>();

function ensureWebSocket(): void {
	if (IS_TAURI || ws) return;

	const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	const url = `${protocol}//${window.location.host}/api/ws`;

	ws = new WebSocket(url);

	ws.onmessage = (event) => {
		try {
			const data = JSON.parse(event.data) as { event: string; payload: unknown };
			const callbacks = wsListeners.get(data.event);
			if (callbacks) {
				for (const cb of callbacks) {
					cb(data.payload);
				}
			}
		} catch (e) {
			console.warn('Failed to parse WebSocket message:', e);
		}
	};

	ws.onclose = () => {
		ws = null;
		// Auto-reconnect after 2 seconds
		if (!wsReconnectTimer) {
			wsReconnectTimer = setTimeout(() => {
				wsReconnectTimer = null;
				if (wsListeners.size > 0) {
					ensureWebSocket();
				}
			}, 2000);
		}
	};

	ws.onerror = () => {
		// onclose will fire after onerror
	};
}

async function onEvent<T>(event: string, cb: EventCallback<T>): Promise<UnlistenFn> {
	if (IS_TAURI) {
		const { listen } = await import('@tauri-apps/api/event');
		return listen<T>(event, (e) => cb(e.payload));
	}

	// Browser mode: register in WebSocket listeners
	if (!wsListeners.has(event)) {
		wsListeners.set(event, new Set());
	}
	wsListeners.get(event)!.add(cb as EventCallback<unknown>);
	ensureWebSocket();

	return () => {
		const set = wsListeners.get(event);
		if (set) {
			set.delete(cb as EventCallback<unknown>);
			if (set.size === 0) {
				wsListeners.delete(event);
			}
		}
	};
}

export const onStreamToken = (cb: (payload: StreamTokenPayload) => void) =>
	onEvent<StreamTokenPayload>('stream-token', cb);

export const onStreamEnd = (cb: (payload: StreamEndPayload) => void) =>
	onEvent<StreamEndPayload>('stream-end', cb);

export const onTextLog = (cb: (payload: TextLogPayload) => void) =>
	onEvent<TextLogPayload>('text-log', cb);

export const onWorldUpdate = (cb: (payload: WorldUpdatePayload) => void) =>
	onEvent<WorldUpdatePayload>('world-update', cb);

export const onLoading = (cb: (payload: LoadingPayload) => void) =>
	onEvent<LoadingPayload>('loading', cb);

export const onThemeUpdate = (cb: (payload: ThemePalette) => void) =>
	onEvent<ThemePalette>('theme-update', cb);

export const onDebugUpdate = (cb: (payload: DebugSnapshot) => void) =>
	onEvent<DebugSnapshot>('debug-update', cb);

export const onSavePicker = (cb: () => void) =>
	onEvent<void>('save-picker', () => cb());
