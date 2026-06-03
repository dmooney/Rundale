import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

// ── Minimal fake WebSocket ───────────────────────────────────────────────
//
// `ipc.ts` creates a real `WebSocket` at module load when any listener is
// registered. We substitute a fake before importing the module so no real
// network traffic is attempted and so we can observe/close behaviour.

interface FakeWs {
	url: string;
	closeCalls: number;
	onopen: ((e: Event) => void) | null;
	onmessage: ((e: MessageEvent) => void) | null;
	onclose: ((e: CloseEvent) => void) | null;
	onerror: ((e: Event) => void) | null;
	simulateOpen: () => void;
	simulateClose: () => void;
}

const openSockets: FakeWs[] = [];

class MockWebSocket implements FakeWs {
	static CONNECTING = 0;
	static OPEN = 1;
	static CLOSING = 2;
	static CLOSED = 3;

	url: string;
	closeCalls = 0;
	onopen: ((e: Event) => void) | null = null;
	onmessage: ((e: MessageEvent) => void) | null = null;
	onclose: ((e: CloseEvent) => void) | null = null;
	onerror: ((e: Event) => void) | null = null;

	constructor(url: string) {
		this.url = url;
		openSockets.push(this);
	}

	close() {
		this.closeCalls += 1;
	}

	simulateOpen() {
		this.onopen?.(new Event('open'));
	}

	simulateClose() {
		// Browsers null handlers before dispatch — mimic by firing whatever
		// the module currently has attached, since the module may null it
		// during `disposeTransport()`.
		const handler = this.onclose;
		if (handler) handler(new CloseEvent('close'));
	}
}

// Stub window.location so ensureWebSocket can build a URL.
beforeEach(() => {
	Object.defineProperty(globalThis, 'WebSocket', {
		configurable: true,
		writable: true,
		value: MockWebSocket,
	});
	openSockets.length = 0;
});

afterEach(() => {
	vi.resetModules();
	vi.useRealTimers();
});

async function loadIpc() {
	// Import fresh so module-level state is isolated per test.
	const mod = await import('./ipc');
	return mod;
}

describe('ipc WebSocket transport lifecycle', () => {
	it('opens a single WebSocket when the first event listener registers', async () => {
		const ipc = await loadIpc();
		await ipc.onTextLog(() => {});
		expect(openSockets.length).toBe(1);
		expect(openSockets[0].url).toContain('/api/ws');
	});

	it('disposeTransport closes the WebSocket and detaches handlers', async () => {
		const ipc = await loadIpc();
		const unlisten = await ipc.onTextLog(() => {});
		expect(openSockets.length).toBe(1);
		const sock = openSockets[0];
		expect(sock.closeCalls).toBe(0);

		ipc.disposeTransport();

		expect(sock.closeCalls).toBe(1);
		expect(sock.onclose).toBeNull();
		expect(sock.onerror).toBeNull();
		expect(sock.onmessage).toBeNull();

		// Dropping the unlisten after disposal must not re-open the socket.
		unlisten();
		expect(openSockets.length).toBe(1);
	});

	it('disposeTransport cancels a pending reconnect timer', async () => {
		vi.useFakeTimers();
		const ipc = await loadIpc();
		await ipc.onTextLog(() => {});
		const sock = openSockets[0];

		// Trigger the reconnect path (mirrors a real socket closing).
		sock.simulateClose();

		// Tear down before the reconnect timer fires.
		ipc.disposeTransport();

		vi.advanceTimersByTime(5_000);
		// The timer must have been cleared — no second socket opened.
		expect(openSockets.length).toBe(1);
	});

	it('is safe to call disposeTransport with no connection', async () => {
		const ipc = await loadIpc();
		expect(() => ipc.disposeTransport()).not.toThrow();
		expect(openSockets.length).toBe(0);
	});

	it('cancels a pending reconnect when the last listener unlistens', async () => {
		vi.useFakeTimers();
		const ipc = await loadIpc();
		const unlisten = await ipc.onTextLog(() => {});
		const sock = openSockets[0];

		// Simulate a drop — module queues a 2s reconnect timer.
		sock.simulateClose();

		// Remove the only listener before the reconnect fires.
		unlisten();

		vi.advanceTimersByTime(5_000);
		expect(openSockets.length).toBe(1); // no zombie second socket
	});

	it('still reconnects if listeners remain when the socket drops', async () => {
		vi.useFakeTimers();
		const ipc = await loadIpc();
		await ipc.onTextLog(() => {});
		const sock = openSockets[0];
		sock.simulateClose();

		// Listener still active, so the 2s timer should reconnect.
		vi.advanceTimersByTime(2_000);
		expect(openSockets.length).toBe(2);
	});
});

describe('ipc reconnect resync (audit M4)', () => {
	it('does not fire onReconnect callbacks on the first connection', async () => {
		const ipc = await loadIpc();
		const resync = vi.fn();
		ipc.onReconnect(resync);
		await ipc.onTextLog(() => {});
		// Initial open — the mount already loaded state, so no resync.
		openSockets[0].simulateOpen();
		expect(resync).not.toHaveBeenCalled();
	});

	it('fires onReconnect callbacks when the socket re-opens after a drop', async () => {
		vi.useFakeTimers();
		const ipc = await loadIpc();
		const resync = vi.fn();
		ipc.onReconnect(resync);
		await ipc.onTextLog(() => {});

		openSockets[0].simulateOpen(); // initial connect
		openSockets[0].simulateClose(); // drop → schedule reconnect
		vi.advanceTimersByTime(2_000); // reconnect timer fires → socket #2
		expect(openSockets.length).toBe(2);

		openSockets[1].simulateOpen(); // reconnect succeeds
		expect(resync).toHaveBeenCalledTimes(1);
	});

	it('stops firing after the listener unsubscribes', async () => {
		vi.useFakeTimers();
		const ipc = await loadIpc();
		const resync = vi.fn();
		const off = ipc.onReconnect(resync);
		await ipc.onTextLog(() => {});
		openSockets[0].simulateOpen();
		off();
		openSockets[0].simulateClose();
		vi.advanceTimersByTime(2_000);
		openSockets[1].simulateOpen();
		expect(resync).not.toHaveBeenCalled();
	});
});

describe('ipc command() HTTP path (audit M6 / M7)', () => {
	beforeEach(() => {
		vi.unstubAllGlobals();
	});
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('rejects with a timeout error when fetch never settles', async () => {
		vi.useFakeTimers();
		// A fetch that rejects with an AbortError once its signal aborts.
		vi.stubGlobal(
			'fetch',
			(_url: string, init?: RequestInit) =>
				new Promise((_resolve, reject) => {
					init?.signal?.addEventListener('abort', () =>
						reject(new DOMException('aborted', 'AbortError')),
					);
				}),
		);
		const ipc = await loadIpc();
		const p = ipc.command('get_world_snapshot');
		const assertion = expect(p).rejects.toThrow(/timeout/);
		await vi.advanceTimersByTimeAsync(31_000);
		await assertion;
	});

	it('times out when the response body stalls after headers arrive', async () => {
		vi.useFakeTimers();
		// Headers resolve immediately, but resp.text() only settles on abort.
		vi.stubGlobal('fetch', (_url: string, init?: RequestInit) =>
			Promise.resolve({
				ok: true,
				text: () =>
					new Promise<string>((_resolve, reject) => {
						init?.signal?.addEventListener('abort', () =>
							reject(new DOMException('aborted', 'AbortError')),
						);
					}),
			} as unknown as Response),
		);
		const ipc = await loadIpc();
		const p = ipc.command('get_map');
		const assertion = expect(p).rejects.toThrow(/timeout/);
		await vi.advanceTimersByTimeAsync(31_000);
		await assertion;
	});

	it('does NOT time out POST mutations (e.g. submit_input with a slow NPC turn)', async () => {
		vi.useFakeTimers();
		let aborted = false;
		// A slow POST that resolves only after 60s and records any abort.
		vi.stubGlobal(
			'fetch',
			(_url: string, init?: RequestInit) =>
				new Promise<Response>((resolve) => {
					init?.signal?.addEventListener('abort', () => {
						aborted = true;
					});
					setTimeout(() => resolve(new Response('')), 60_000);
				}),
		);
		const ipc = await loadIpc();
		const p = ipc.command('submit_input', { text: 'hello', addressedTo: [] });
		await vi.advanceTimersByTimeAsync(60_000);
		await expect(p).resolves.toBeUndefined();
		expect(aborted).toBe(false);
	});

	it('getAuthStatus returns parsed status in web mode', async () => {
		vi.stubGlobal(
			'fetch',
			vi
				.fn()
				.mockResolvedValue(
					new Response(
						JSON.stringify({ oauth_enabled: true, logged_in: false }),
					),
				),
		);
		const ipc = await loadIpc();
		const status = await ipc.getAuthStatus();
		expect(status).toEqual({ oauth_enabled: true, logged_in: false });
	});

	it('getAuthStatus returns null when the request fails', async () => {
		vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));
		const ipc = await loadIpc();
		expect(await ipc.getAuthStatus()).toBeNull();
	});
});
