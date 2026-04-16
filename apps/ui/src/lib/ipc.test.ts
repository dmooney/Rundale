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
		value: MockWebSocket
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
