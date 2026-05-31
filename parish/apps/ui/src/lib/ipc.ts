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
	SaveFileInfo,
	SaveState,
	DemoContextSnapshot,
	DemoConfigPayload,
	BugContext,
	BugReportResult,
	AuthStatus
} from './types';

// ── Transport detection ─────────────────────────────────────────────────────

const IS_TAURI = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/**
 * Hard ceiling for a single HTTP command in web mode.
 *
 * The synchronous `/api/*` endpoints are all fast reads or fire-and-forget
 * posts (NPC inference streams back over the WebSocket, not the POST body), so
 * a request that runs this long means the server is wedged. Without a bound a
 * hung server leaves the mount-time `Promise.allSettled` pending forever and
 * the UI shows a permanent partial load with no error (audit M6). On abort we
 * reject so the caller's error path runs.
 */
const COMMAND_TIMEOUT_MS = 30_000;

// ── Commands ────────────────────────────────────────────────────────────────

export async function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
	if (IS_TAURI) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<T>(name, args);
	}
	// Web mode: REST API
	const endpoint = `/api/${name.replace(/^get_/, '').replace(/_/g, '-')}`;
	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS);
	let resp: Response;
	try {
		resp = await fetch(endpoint, {
			method: args ? 'POST' : 'GET',
			headers: args ? { 'Content-Type': 'application/json' } : {},
			body: args ? JSON.stringify(args) : undefined,
			signal: controller.signal
		});
	} catch (e) {
		if (controller.signal.aborted) {
			throw new Error(`API timeout after ${COMMAND_TIMEOUT_MS}ms: ${name}`);
		}
		throw e;
	} finally {
		clearTimeout(timer);
	}
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

/** Toggles the desktop window's fullscreen state. Tauri-only (F11). */
export const toggleFullscreen = () => command<boolean>('toggle_fullscreen');

// ── Mod selection ────────────────────────────────────────────────────────────

export const getMods = (): Promise<ModEntry[]> => {
	if (IS_TAURI) {
		return Promise.resolve([]);
	}
	return fetch('/api/mods').then((r) => r.json());
};

export const switchMod = (modId: string): Promise<{ ok: boolean; error?: string }> => {
	if (IS_TAURI) {
		return Promise.resolve({ ok: false, error: 'not supported in desktop mode' });
	}
	return fetch('/api/mods/switch', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ mod_id: modId })
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

// ── Demo / auto-player commands ──────────────────────────────────────────────

export const getDemoConfig = () => command<DemoConfigPayload>('get_demo_config');

export const getDemoContext = () => command<DemoContextSnapshot>('get_demo_context');

export const getLlmPlayerAction = (ctx: DemoContextSnapshot) =>
	command<string>('get_llm_player_action', { ctx });

// ── Screenshot commands ─────────────────────────────────────────────────────

export interface ScreenshotInfo {
	/** Absolute filesystem path to the PNG written by the backend. */
	path: string;
	/** ISO-8601 UTC timestamp the file was written (`YYYY-MM-DDTHH:MM:SSZ`). */
	taken_at: string;
	/** Size of the PNG payload in bytes. */
	size_bytes: number;
}

/**
 * Persists a screenshot captured by `captureScreen()` (in `lib/screenshot.ts`).
 *
 * `dataUrl` must be a `data:image/png;base64,...` string. Tauri-only: the
 * web server returns 501 since the headless backend has no DOM to capture.
 */
export const saveScreenshot = (dataUrl: string) =>
	command<ScreenshotInfo>('save_screenshot', { dataUrl });

// ── Bug reporting ─────────────────────────────────────────────────────────────

/**
 * Files a bug report — bundles a screenshot, recent logs, and current game
 * state into a GitHub issue (or an on-disk bundle in dry-run / no-token mode).
 *
 * Keys are camelCase so the single `command()` adapter works across both
 * transports: Tauri maps `screenshotDataUrl` → the `screenshot_data_url`
 * argument, and the web route's `BugReportRequest` uses `rename_all =
 * "camelCase"`.
 */
export const submitBugReport = (args: {
	title: string;
	description: string;
	screenshotDataUrl?: string;
	context?: BugContext;
}) => command<BugReportResult>('submit_bug_report', args);

/**
 * Reads metadata for the most recently captured screenshot, or `null` if
 * none has been captured this session (or the cached file was deleted).
 */
export const getLatestScreenshot = () =>
	command<ScreenshotInfo | null>('get_latest_screenshot');

/**
 * Sends the result of an agent-triggered screenshot back to the MCP bridge.
 *
 * Called by the frontend after it receives a `request-screenshot` event
 * (via `onRequestScreenshot`) and completes the capture. The bridge handler
 * that emitted the event is waiting on a oneshot channel keyed by
 * `request_id`; this call unblocks it so it can return `ScreenshotInfo` to
 * the MCP client.
 *
 * Only meaningful in Tauri mode — the server returns 501 for take-screenshot
 * and never emits the event, so this is never called in web mode.
 */
export const notifyScreenshotCaptured = (request_id: string, info: ScreenshotInfo) =>
	command<void>('notify_screenshot_captured', { request_id, info });

/**
 * Reports a screenshot capture failure back to the MCP bridge so it can
 * return an error to the MCP client immediately rather than waiting for the
 * 15-second timeout.
 *
 * Call this whenever `captureScreen()` or `saveScreenshot()` throws inside
 * the `onRequestScreenshot` handler.
 */
export const notifyScreenshotError = (request_id: string, error: string) =>
	command<void>('notify_screenshot_error', { request_id, error });

export interface RequestScreenshotPayload {
	request_id: string;
}

/** Registers a handler for agent-triggered screenshot requests. */
export const onRequestScreenshot = (cb: (payload: RequestScreenshotPayload) => void) =>
	onEvent<RequestScreenshotPayload>('request-screenshot', cb);

// ── Events ──────────────────────────────────────────────────────────────────

type UnlistenFn = () => void;
type EventCallback<T> = (payload: T) => void;

// WebSocket state for browser mode
let ws: WebSocket | null = null;
let wsReconnectTimer: ReturnType<typeof setTimeout> | null = null;
const wsListeners = new Map<string, Set<EventCallback<unknown>>>();

// Reconnect-resync hooks. Any event emitted while the socket was down is lost
// (unlike Tauri's `listen`, which never disconnects), so a dropped
// `stream-end` could leave the HUD desynced — e.g. `streamingActive` stuck
// true. Callers register here to re-fetch authoritative state when the socket
// re-opens after a drop (audit M4). The very first connection does NOT fire
// these — the mount already loads initial state.
const wsReconnectListeners = new Set<() => void>();
let wsHasConnected = false;

/**
 * Registers a callback fired after the browser WebSocket reconnects following a
 * drop (never on the initial connection). Use it to re-fetch world snapshot /
 * map / npcs so events missed during the gap can't leave the UI out of sync.
 * No-op in Tauri (the desktop transport never disconnects). Returns an
 * unsubscribe function.
 */
export function onReconnect(cb: () => void): UnlistenFn {
	if (IS_TAURI) return () => {};
	wsReconnectListeners.add(cb);
	return () => {
		wsReconnectListeners.delete(cb);
	};
}

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
	socket.onopen = () => {
		if (wsHasConnected) {
			// This is a reconnect, not the first connection — replay-resync any
			// state lost during the gap. Snapshot the set so a callback that
			// unsubscribes mid-flush can't perturb iteration.
			for (const cb of [...wsReconnectListeners]) {
				try {
					cb();
				} catch (e) {
					console.warn('Reconnect resync callback failed:', e);
				}
			}
		}
		wsHasConnected = true;
	};

	socket.onmessage = (event) => {
		try {
			const data = JSON.parse(event.data) as { event: string; payload: unknown };
			const callbacks = wsListeners.get(data.event);
			if (callbacks) {
				// Snapshot before iterating: a callback may unlisten (and thus
				// mutate this Set) during dispatch.
				for (const cb of [...callbacks]) {
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
		ws.onopen = null;
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
	// Reset so the next mount's first connection isn't treated as a reconnect.
	wsHasConnected = false;
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

// ── Setup overlay events ────────────────────────────────────────────────────

export function isTauri(): boolean {
	return IS_TAURI;
}

export interface SetupStatusPayload {
	message: string;
}
export interface SetupProgressPayload {
	/** Bytes downloaded so far across discovered Ollama pull artifacts. */
	completed: number;
	/** Total bytes expected across discovered Ollama pull artifacts. */
	total: number;
}
export interface SetupDonePayload {
	success: boolean;
	error: string;
}
export interface SetupSnapshot {
	current_message: string;
	messages: string[];
	completed: number;
	total: number;
	done: boolean;
	success: boolean | null;
	error: string;
	needs_onboarding: boolean;
}

export const getSetupSnapshot = () => command<SetupSnapshot>('get_setup_snapshot');

export const onSetupStatus = (cb: (payload: SetupStatusPayload) => void) =>
	onEvent<SetupStatusPayload>('setup-status', cb);

export const onSetupProgress = (cb: (payload: SetupProgressPayload) => void) =>
	onEvent<SetupProgressPayload>('setup-progress', cb);

export const onSetupDone = (cb: (payload: SetupDonePayload) => void) =>
	onEvent<SetupDonePayload>('setup-done', cb);

export const onSetupNeedsOnboarding = (cb: (payload: SetupStatusPayload) => void) =>
	onEvent<SetupStatusPayload>('setup-needs-onboarding', cb);

// ── BYOK onboarding commands ────────────────────────────────────────────────

export interface ByokCategoryOverride {
	provider?: string;
	model?: string;
	base_url?: string;
}

export interface SetProviderConfigArgs {
	provider: string;
	base_url?: string;
	model?: string;
	api_key?: string;
	category_overrides?: Record<string, ByokCategoryOverride>;
}

export interface ValidateProviderConfigArgs {
	provider: string;
	base_url?: string;
	api_key?: string;
}

export type ValidationOutcome =
	| { kind: 'ok' }
	| { kind: 'auth_failed'; status: number; body_excerpt: string }
	| { kind: 'not_found'; status: number; body_excerpt: string }
	| { kind: 'rate_limited'; status: number; retry_after_secs: number | null }
	| { kind: 'network'; message: string }
	| { kind: 'unexpected'; status: number; body_excerpt: string };

export interface GetProviderConfigResult {
	provider: string;
	model: string;
	base_url: string;
	has_api_key: boolean;
	has_env_key: boolean;
}

export const setProviderConfig = (args: SetProviderConfigArgs) =>
	command<void>('set_provider_config', { args });

export const validateProviderConfig = (args: ValidateProviderConfigArgs) =>
	command<ValidationOutcome>('validate_provider_config', { args });

export const getProviderConfig = () =>
	command<GetProviderConfigResult>('get_provider_config');

export const clearProviderConfig = () => command<void>('clear_provider_config');

export const listByokEnvKeys = () =>
	command<Record<string, boolean>>('list_byok_env_keys');

export interface ProviderPresetOption {
	key: string;
	label: string;
	dialogue: string | null;
	simulation: string | null;
	intent: string | null;
	reaction: string | null;
}

export const listPresetModels = () =>
	command<Record<string, ProviderPresetOption[]>>('list_preset_models');

export interface AvailableProviderInfo {
	id: string;
	display_name: string;
	blurb: string | null;
	signup_url: string | null;
	needs_base_url: boolean;
	keyless: boolean;
	featured: boolean;
}

export interface AvailableProvidersResponse {
	featured: AvailableProviderInfo[];
	other: AvailableProviderInfo[];
}

export const listAvailableProviders = () =>
	command<AvailableProvidersResponse>('list_available_providers');

/**
 * Bindings for the local-inference onboarding flow (vllm-mlx on macOS).
 *
 * `OnboardingChoice` is serialized kebab-case by the Rust enum:
 *   "configured" | "local-recommended" | "local-low-mem" | "local-unavailable"
 *
 * `LocalSetupArgs.variant`:
 *   - "two-slot"   — 14B Dialogue + 1.5B small-slot. Recommended on Mac ≥ 16 GB.
 *   - "small-only" — 1.5B for everything. Mac < 16 GB; degraded quality.
 */
export type OnboardingChoice =
	| 'configured'
	| 'local-recommended'
	| 'local-low-mem'
	| 'local-unavailable';

export interface OnboardingOptions {
	choice: OnboardingChoice;
	ram_gb: number;
}

export interface LocalSetupArgs {
	variant: 'two-slot' | 'small-only';
}

export const getOnboardingOptions = () =>
	command<OnboardingOptions>('get_onboarding_options');

export const startLocalInferenceSetup = (args: LocalSetupArgs) =>
	command<void>('start_local_inference_setup', { args });
