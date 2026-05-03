/**
 * WebGPU inference bridge — wires WebSocket `inference-request` events from
 * the server to the local WebLLM engine and streams results back.
 *
 * Lifecycle:
 *   1. Call `startWebGpuBridge()` once after the WebSocket connects.
 *   2. The bridge listens for `inference-request` events.
 *   3. For each request, it runs WebLLM inference and sends back
 *      `inference-token`, `inference-done`, or `inference-error` frames.
 *   4. Call the returned unlisten function to tear down the bridge.
 */

import { onInferenceRequest, sendWebSocketMessage, type InferenceRequestPayload } from './ipc';
import { webGpuProvider } from './webgpu-provider';

/** Starts the WebGPU bridge. Returns a cleanup function. */
export function startWebGpuBridge(): () => void {
	let unlistenPromise: Promise<() => void> | null = null;

	// Active inference cancellation flags keyed by request id.
	const cancelled = new Set<number>();

	async function handleRequest(req: InferenceRequestPayload): Promise<void> {
		if (!webGpuProvider.isLoaded) {
			sendWebSocketMessage({
				type: 'inference-error',
				id: req.id,
				error: 'WebGPU model not loaded — select a model in the Inference panel first'
			});
			return;
		}

		try {
			const text = await webGpuProvider.generate(
				req.prompt,
				req.system ?? null,
				req.max_tokens ?? null,
				req.temperature ?? null,
				(token) => {
					if (!cancelled.has(req.id)) {
						sendWebSocketMessage({ type: 'inference-token', id: req.id, token });
					}
				}
			);

			if (!cancelled.has(req.id)) {
				sendWebSocketMessage({ type: 'inference-done', id: req.id, text });
			}
		} catch (err) {
			if (!cancelled.has(req.id)) {
				sendWebSocketMessage({
					type: 'inference-error',
					id: req.id,
					error: err instanceof Error ? err.message : String(err)
				});
			}
		} finally {
			cancelled.delete(req.id);
		}
	}

	unlistenPromise = onInferenceRequest((req) => {
		// Fire-and-forget — each request is independent.
		void handleRequest(req);
	});

	return () => {
		// Cancel all in-flight requests so their token callbacks are no-ops.
		// The server will time out and emit an error to the game log.
		void unlistenPromise?.then((unlisten) => unlisten());
		unlistenPromise = null;
	};
}
