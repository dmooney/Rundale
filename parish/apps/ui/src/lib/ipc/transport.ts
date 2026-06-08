/**
 * IPC transport core — the single seam shared by every command family
 * (#1200 TD-054).
 *
 * In Tauri mode, uses `@tauri-apps/api` invoke/listen.
 * In browser mode, uses fetch for commands and WebSocket for events.
 * All command families import `command` / `onEvent` from here so there is
 * exactly one transport adapter; `$lib/ipc` re-exports everything so existing
 * import paths are unchanged.
 */

// ── Transport detection ─────────────────────────────────────────────────────

export const IS_TAURI =
	typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function isTauri(): boolean {
	return IS_TAURI;
}

/**
 * Hard ceiling for a single HTTP **read** in web mode.
 *
 * Bounds only argless GET reads (world snapshot, map, npcs, theme, ui-config,
 * debug snapshot — the mount-time load). Without a bound a hung server leaves
 * the mount `Promise.allSettled` pending forever and the UI shows a permanent
 * partial load with no error (audit M6). On abort we reject so the caller's
 * error path runs.
 *
 * POST mutations are deliberately NOT bounded: `submit_input` awaits the full
 * server-side game loop (NPC inference can legitimately run far longer than
 * this even though tokens stream over the WebSocket), and provider-config /
 * bug-report posts dial external services. Aborting those mid-flight would
 * surface a spurious "timeout", re-enable the input, and risk duplicate turns
 * while the backend is still working.
 */
export const COMMAND_TIMEOUT_MS = 30_000;

// ── Commands ────────────────────────────────────────────────────────────────

export async function command<T>(
	name: string,
	args?: Record<string, unknown>,
): Promise<T> {
	if (IS_TAURI) {
		const { invoke } = await import('@tauri-apps/api/core');
		return invoke<T>(name, args);
	}
	// Web mode: REST API
	const endpoint = `/api/${name.replace(/^get_/, '').replace(/_/g, '-')}`;
	// Bound reads only (see COMMAND_TIMEOUT_MS): a GET has no args, a mutation does.
	const controller = args ? null : new AbortController();
	const timer = controller
		? setTimeout(() => controller.abort(), COMMAND_TIMEOUT_MS)
		: null;
	try {
		let resp: Response;
		try {
			resp = await fetch(endpoint, {
				method: args ? 'POST' : 'GET',
				headers: args ? { 'Content-Type': 'application/json' } : {},
				body: args ? JSON.stringify(args) : undefined,
				signal: controller?.signal,
			});
		} catch (e) {
			if (controller?.signal.aborted) {
				throw new Error(`API timeout after ${COMMAND_TIMEOUT_MS}ms: ${name}`);
			}
			throw e;
		}
		if (!resp.ok) {
			throw new Error(`API error: ${resp.status} ${resp.statusText}`);
		}
		// Read the body under the same timer: if headers arrive but the body
		// stalls (proxy / partially-hung response), the abort signal cancels
		// resp.text() too, so the timeout bounds the whole read — not just the
		// headers. submit_input returns 200 with no body; the two-step cast
		// makes the unsoundness explicit and searchable rather than hiding it (#755).
		let text: string;
		try {
			text = await resp.text();
		} catch (e) {
			if (controller?.signal.aborted) {
				throw new Error(`API timeout after ${COMMAND_TIMEOUT_MS}ms: ${name}`);
			}
			throw e;
		}
		if (!text) return undefined as unknown as T;
		return JSON.parse(text) as T;
	} finally {
		if (timer) clearTimeout(timer);
	}
}

// ── Events ──────────────────────────────────────────────────────────────────

export type UnlistenFn = () => void;
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
			const data = JSON.parse(event.data) as {
				event: string;
				payload: unknown;
			};
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

export async function onEvent<T>(
	event: string,
	cb: EventCallback<T>,
): Promise<UnlistenFn> {
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
