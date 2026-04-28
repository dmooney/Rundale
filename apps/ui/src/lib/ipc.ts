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
	StreamTurnEndPayload,
	StreamEndPayload,
	TextLogPayload,
	NpcReactionPayload,
	WorldUpdatePayload,
	LoadingPayload,
	TravelStartPayload,
	DebugSnapshot,
	SaveFileInfo,
	SaveState
} from './types';

// ── Transport detection ─────────────────────────────────────────────────────

const IS_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

// ── Commands ────────────────────────────────────────────────────────────────

export async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
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
	// submit_input returns 200 with no body; the two-step cast makes the
	// unsoundness explicit and searchable rather than hiding it (#755).
	const text = await resp.text();
	if (!text) return undefined as unknown as T;
	return JSON.parse(text) as T;
}

export const getWorldSnapshot = () => command<WorldSnapshot>('get_world_snapshot');

export const getMap = () => command<MapData>('get_map');

export const getNpcsHere = () => command<NpcInfo[]>('get_npcs_here');

export const getTheme = () => command<ThemePalette>('get_theme');

export const submitInput = (text: string, addressedTo: string[] = []) =>
	command<void>('submit_input', { text, addressedTo });

export const getDebugSnapshot = () => command<DebugSnapshot>('get_debug_snapshot');

export const getUiConfig = () => command<UiConfig>('get_ui_config');

// ── Persistence commands ────────────────────────────────────────────────────

export const discoverSaveFiles = () => command<SaveFileInfo[]>('discover_save_files');

export const saveGame = () => command<string>('save_game', {});

export const loadBranch = (filePath: string, branchId: number) =>
	command<void>('load_branch', { filePath, branchId });

export const createBranch = (name: string, parentBranchId: number) =>
	command<string>('create_branch', { name, parentBranchId });

export const newSaveFile = () => command<void>('new_save_file', {});

export const newGame = () => command<void>('new_game', {});

export const getSaveState = () => command<SaveState>('get_save_state');

// ── Reaction commands ──────────────────────────────────────────────────────

export const reactToMessage = (npcName: string, messageSnippet: string, emoji: string) =>
	command<void>('react_to_message', { npcName, messageSnippet, emoji });

// ── Events ──────────────────────────────────────────────────────────────────

type UnlistenFn = () => void;
type EventCallback<T> = (payload: T) => void;

// WebSocket state for browser mode
let ws: WebSocket | null = null;
let wsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
const wsListeners = new Map<string, Set<EventCallback<unknown>>>();

function clearReconnectTimer(): void {
	if (wsReconnectTimer !== null) {
		clearTimeout(wsReconnectTimer);
		wsReconnectTimer = null;
	}
}

async function mintSessionToken(): Promise<string | null> {
	// #377 — ws_handler rejects upgrades without a valid HMAC token minted by
	// /api/session-init. In debug+loopback the server bypasses this, so an
	// empty token string is fine; in release the token is required.
	try {
		const resp = await fetch('/api/session-init', { method: 'POST' });
		if (!resp.ok) return null;
		const body = (await resp.json()) as { token?: string };
		return body.token ?? null;
	} catch {
		return null;
	}
}

function isLoopbackHost(): boolean {
	const h = window.location.hostname;
	return h === 'localhost' || h === '127.0.0.1' || h === '::1';
}

function ensureWebSocket(): void {
	if (IS_TAURI || ws) return;

	const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
	const baseUrl = `${protocol}//${window.location.host}/api/ws`;

	// Loopback bypass mirrors crates/parish-server/src/ws.rs — in debug
	// builds the server accepts WS upgrades from 127.0.0.1 / localhost
	// without a token, so we skip the /api/session-init round-trip both
	// for developer convenience and so vitest + Playwright don't need to
	// mock the endpoint. Any non-loopback origin (CF tunnel, prod) must
	// mint a token first.
	if (isLoopbackHost()) {
		ws = new WebSocket(baseUrl);
		attachHandlers(ws);
		return;
	}

	void mintSessionToken().then((token) => {
		if (ws) return; // another caller raced us
		if (!token) {
			console.error('Session token mint failed — not opening WebSocket');
			return;
		}
		const url = `${baseUrl}?token=${encodeURIComponent(token)}`;
		ws = new WebSocket(url);
		attachHandlers(ws);
	});
}

function attachHandlers(socket: WebSocket): void {
	socket.onmessage = (event) => {
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

	socket.onclose = () => {
		ws = null;
		// Auto-reconnect after 2 seconds, but only if we still have
		// listeners expecting events. If the page already tore down its
		// listeners, bail out instead of reconnecting to nothing.
		if (wsReconnectTimer === null && wsListeners.size > 0) {
			wsReconnectTimer = setTimeout(() => {
				wsReconnectTimer = null;
				if (wsListeners.size > 0) {
					ensureWebSocket();
				}
			}, 2000);
		}
	};

	socket.onerror = () => {
		// onclose will fire after onerror
	};
}

/**
 * Tear down the browser-mode WebSocket transport.
 *
 * Clears the pending reconnect timer (if any) and closes the socket.
 * Safe to call multiple times and in Tauri mode (no-op). The page
 * should call this from `onDestroy` to prevent orphaned connections
 * and reconnect timers after navigation.
 */
export function disposeTransport(): void {
	if (IS_TAURI) return;
	clearReconnectTimer();
	if (ws) {
		// Detach handlers so the `onclose` reconnect path doesn't fire.
		ws.onclose = null;
		ws.onerror = null;
		ws.onmessage = null;
		try {
			ws.close();
		} catch {
			// Ignore — already closing/closed.
		}
		ws = null;
	}
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
		// When no listeners remain, cancel any pending reconnect so we
		// don't open a zombie socket after the page has torn down.
		if (wsListeners.size === 0) {
			clearReconnectTimer();
		}
	};
}

export const onStreamToken = (cb: (payload: StreamTokenPayload) => void) =>
	onEvent<StreamTokenPayload>('stream-token', cb);

export const onStreamTurnEnd = (cb: (payload: StreamTurnEndPayload) => void) =>
	onEvent<StreamTurnEndPayload>('stream-turn-end', cb);

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

export interface ThemeSwitchPayload {
	name: string;
	mode: string;
}
export const onThemeSwitch = (cb: (payload: ThemeSwitchPayload) => void) =>
	onEvent<ThemeSwitchPayload>('theme-switch', cb);

export interface TilesSwitchPayload {
	id: string;
}
export const onTilesSwitch = (cb: (payload: TilesSwitchPayload) => void) =>
	onEvent<TilesSwitchPayload>('tiles-switch', cb);

export const onDebugUpdate = (cb: (payload: DebugSnapshot) => void) =>
	onEvent<DebugSnapshot>('debug-update', cb);

export const onSavePicker = (cb: () => void) =>
	onEvent<void>('save-picker', () => cb());

export const onToggleFullMap = (cb: () => void) =>
	onEvent<void>('toggle-full-map', () => cb());

export const onOpenDesigner = (cb: () => void) =>
	onEvent<void>('open-designer', () => cb());

export const onNpcReaction = (cb: (payload: NpcReactionPayload) => void) =>
	onEvent<NpcReactionPayload>('npc-reaction', cb);

export const onTravelStart = (cb: (payload: TravelStartPayload) => void) =>
	onEvent<TravelStartPayload>('travel-start', cb);
