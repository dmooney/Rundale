/**
 * Tests for the browser-side WebGPU bridge.
 *
 * The bridge subscribes to `webgpu-generate` events from the server, runs a
 * generation through `engine.generate`, and posts replies back over the WS
 * via `sendWsFrame`. We mock both `engine` and `ipc` so the test exercises
 * the bridge wiring without a real model or socket.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const generateMock = vi.fn();
const isWebGpuAvailableMock = vi.fn();
const sendWsFrameMock = vi.fn();

// Capture the listener registered by the bridge so the test can fire
// synthetic `webgpu-generate` payloads at it.
let registeredListener: ((p: unknown) => void) | null = null;
const onWebGpuGenerateMock = vi.fn(async (cb: (p: unknown) => void) => {
	registeredListener = cb;
	return () => {
		registeredListener = null;
	};
});

vi.mock('../ipc', () => ({
	IS_TAURI: false,
	onWebGpuGenerate: (cb: (p: unknown) => void) => onWebGpuGenerateMock(cb),
	sendWsFrame: (event: string, payload: unknown) => sendWsFrameMock(event, payload)
}));

vi.mock('./engine', () => ({
	generate: (...args: unknown[]) => generateMock(...args),
	isWebGpuAvailable: () => isWebGpuAvailableMock()
}));

import { startWebGpuBridge, stopWebGpuBridge } from './bridge';

describe('startWebGpuBridge', () => {
	beforeEach(() => {
		generateMock.mockReset();
		isWebGpuAvailableMock.mockReset();
		sendWsFrameMock.mockReset();
		onWebGpuGenerateMock.mockClear();
		registeredListener = null;
		isWebGpuAvailableMock.mockReturnValue(true);
	});

	afterEach(() => {
		stopWebGpuBridge();
	});

	it('subscribes exactly once even when called repeatedly', async () => {
		await startWebGpuBridge();
		await startWebGpuBridge();
		expect(onWebGpuGenerateMock).toHaveBeenCalledTimes(1);
	});

	it('forwards a successful generation as webgpu-end', async () => {
		generateMock.mockResolvedValueOnce('Sure now and that is the truth.');
		await startWebGpuBridge();
		registeredListener?.({
			request_id: 7,
			model: 'onnx-community/gemma-4-E2B-it-ONNX',
			prompt: 'hi',
			system: null,
			max_tokens: null,
			temperature: null,
			streaming: false,
			json_mode: false
		});
		// Allow the inner promise to resolve.
		await Promise.resolve();
		await Promise.resolve();
		expect(sendWsFrameMock).toHaveBeenCalledWith('webgpu-end', {
			request_id: 7,
			full_text: 'Sure now and that is the truth.'
		});
	});

	it('reports webgpu-error when WebGPU is unavailable', async () => {
		isWebGpuAvailableMock.mockReturnValue(false);
		await startWebGpuBridge();
		registeredListener?.({
			request_id: 9,
			model: 'm',
			prompt: 'p',
			system: null,
			max_tokens: null,
			temperature: null,
			streaming: false,
			json_mode: false
		});
		await Promise.resolve();
		expect(generateMock).not.toHaveBeenCalled();
		expect(sendWsFrameMock).toHaveBeenCalledWith(
			'webgpu-error',
			expect.objectContaining({
				request_id: 9,
				message: expect.stringContaining('WebGPU')
			})
		);
	});

	it('forwards generation errors as webgpu-error', async () => {
		generateMock.mockRejectedValueOnce(new Error('out of memory'));
		await startWebGpuBridge();
		registeredListener?.({
			request_id: 12,
			model: 'm',
			prompt: 'p',
			system: null,
			max_tokens: null,
			temperature: null,
			streaming: false,
			json_mode: false
		});
		await Promise.resolve();
		await Promise.resolve();
		expect(sendWsFrameMock).toHaveBeenCalledWith('webgpu-error', {
			request_id: 12,
			message: 'out of memory'
		});
	});

	it('passes a streaming callback when streaming=true and forwards each token', async () => {
		generateMock.mockImplementationOnce(async ({ onToken }: { onToken?: (t: string) => void }) => {
			onToken?.('alpha');
			onToken?.('beta');
			onToken?.('');
			onToken?.('gamma');
			return 'alphabetagamma';
		});
		await startWebGpuBridge();
		registeredListener?.({
			request_id: 1,
			model: 'm',
			prompt: 'p',
			system: 'sys',
			max_tokens: 64,
			temperature: 0.5,
			streaming: true,
			json_mode: false
		});
		await Promise.resolve();
		await Promise.resolve();

		// Expect three webgpu-token frames (empty deltas are skipped) and one webgpu-end.
		const tokenFrames = sendWsFrameMock.mock.calls.filter((c) => c[0] === 'webgpu-token');
		const endFrames = sendWsFrameMock.mock.calls.filter((c) => c[0] === 'webgpu-end');
		expect(tokenFrames.map((c) => c[1].delta)).toEqual(['alpha', 'beta', 'gamma']);
		expect(endFrames).toHaveLength(1);
		expect(endFrames[0][1]).toEqual({ request_id: 1, full_text: 'alphabetagamma' });
	});

	it('omits the streaming callback when streaming=false', async () => {
		let receivedOnToken: ((t: string) => void) | undefined = () => {};
		generateMock.mockImplementationOnce(async (opts: { onToken?: (t: string) => void }) => {
			receivedOnToken = opts.onToken;
			return 'done';
		});
		await startWebGpuBridge();
		registeredListener?.({
			request_id: 2,
			model: 'm',
			prompt: 'p',
			system: null,
			max_tokens: null,
			temperature: null,
			streaming: false,
			json_mode: false
		});
		await Promise.resolve();
		await Promise.resolve();
		expect(receivedOnToken).toBeUndefined();
	});
});
