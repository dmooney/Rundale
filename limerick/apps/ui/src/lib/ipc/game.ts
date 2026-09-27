/**
 * Core gameplay IPC: world reads, player input, mod selection, auth status,
 * reactions, the demo auto-player, and the real-time event subscriptions
 * (#1200 TD-054). Thin wrappers over the shared transport seam.
 */

import type {
	WorldSnapshot,
	MapData,
	NpcInfo,
	ThemePalette,
	UiConfig,
	ModEntry,
	StreamTokenPayload,
	StreamTurnEndPayload,
	StreamEndPayload,
	TextLogPayload,
	NpcReactionPayload,
	WorldUpdatePayload,
	LoadingPayload,
	TravelStartPayload,
	DebugSnapshot,
	DemoContextSnapshot,
	DemoConfigPayload,
	AuthStatus,
	DialogueCorrectedPayload,
	ReconnectState,
} from '../types';
import { command, onEvent, IS_TAURI, COMMAND_TIMEOUT_MS } from './transport';

export const getWorldSnapshot = () =>
	command<WorldSnapshot>('get_world_snapshot');

export const getMap = () => command<MapData>('get_map');

export const getNpcsHere = () => command<NpcInfo[]>('get_npcs_here');

export const getReconnectState = () =>
	command<ReconnectState>('get_reconnect_state');

export const getTheme = () => command<ThemePalette>('get_theme');

export const submitInput = (text: string, addressedTo: string[] = []) =>
	command<void>('submit_input', { text, addressedTo });

export const getDebugSnapshot = () =>
	command<DebugSnapshot>('get_debug_snapshot');

export const getUiConfig = () => command<UiConfig>('get_ui_config');

/** Toggles the desktop window's fullscreen state. Tauri-only (F11). */
export const toggleFullscreen = () => command<boolean>('toggle_fullscreen');

// ── Mod selection ────────────────────────────────────────────────────────────

export const getMods = (): Promise<ModEntry[]> => {
	if (IS_TAURI) {
		return Promise.resolve([]);
	}
	return fetch('/api/mods').then((r) => r.json());
};

export const switchMod = (
	modId: string,
): Promise<{ ok: boolean; error?: string }> => {
	if (IS_TAURI) {
		return Promise.resolve({
			ok: false,
			error: 'not supported in desktop mode',
		});
	}
	return fetch('/api/mods/switch', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ mod_id: modId }),
	}).then((r) => r.json());
};

// ── Auth status (web-only OAuth UI) ──────────────────────────────────────────

/**
 * Fetches OAuth status for the web server's optional Google sign-in indicator.
 *
 * Forks inside the seam (like getMods/switchMod): Tauri desktop has no auth
 * server, so it resolves to `null`; web mode hits `GET /api/auth/status` — a
 * slash route that doesn't fit `command()`'s kebab-cased `/api/<name>` mapping.
 * Returns `null` on any failure; the auth indicator is non-critical chrome.
 * Centralising it here keeps AuthStatus.svelte off the raw transport (the
 * "don't fork transports in components" seam rule).
 */
export const getAuthStatus = async (): Promise<AuthStatus | null> => {
	if (IS_TAURI) return null;
	// Bound the fetch like command() does (M6): a wedged server must not hang
	// the mount flow on this optional call. Any failure (including abort)
	// resolves to null rather than throwing — the auth UI is non-critical.
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS);
	try {
		const resp = await fetch('/api/auth/status', { signal: controller.signal });
		return resp.ok ? ((await resp.json()) as AuthStatus) : null;
	} catch {
		return null;
	} finally {
		clearTimeout(timer);
	}
};

// ── Reaction commands ──────────────────────────────────────────────────────

export const reactToMessage = (
	npcName: string,
	messageSnippet: string,
	emoji: string,
) => command<void>('react_to_message', { npcName, messageSnippet, emoji });

// ── Demo / auto-player commands ──────────────────────────────────────────────

export const getDemoConfig = () =>
	command<DemoConfigPayload>('get_demo_config');

export const getDemoContext = () =>
	command<DemoContextSnapshot>('get_demo_context');

export const getLlmPlayerAction = (ctx: DemoContextSnapshot) =>
	command<string>('get_llm_player_action', { ctx });

// ── Events ────────────────────────────────────────────────────────────────

export const onStreamToken = (cb: (payload: StreamTokenPayload) => void) =>
	onEvent<StreamTokenPayload>('stream-token', cb);

export const onStreamTurnEnd = (cb: (payload: StreamTurnEndPayload) => void) =>
	onEvent<StreamTurnEndPayload>('stream-turn-end', cb);

export const onDialogueCorrected = (
	cb: (payload: DialogueCorrectedPayload) => void,
) => onEvent<DialogueCorrectedPayload>('dialogue-corrected', cb);

export const onStreamEnd = (cb: (payload: StreamEndPayload) => void) =>
	onEvent<StreamEndPayload>('stream-end', cb);

export const onTextLog = (cb: (payload: TextLogPayload) => void) =>
	onEvent<TextLogPayload>('text-log', cb);

export const onWorldUpdate = (cb: (payload: WorldUpdatePayload) => void) =>
	onEvent<WorldUpdatePayload>('world-update', cb);

/**
 * Fires immediately before the authoritative world update for a new game or
 * loaded branch. Consumers must discard view state derived from the previous
 * game context rather than carrying it into the replacement world.
 */
export const onGameContextReset = (cb: () => void) =>
	onEvent<void>('game-context-reset', () => cb());

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
