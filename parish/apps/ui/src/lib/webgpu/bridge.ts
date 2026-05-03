/**
 * Browser-side WebGPU bridge.
 *
 * Subscribes to `webgpu-generate` frames pushed by the server (one per
 * inference request the server made through `Provider::WebGpu`), runs the
 * model locally via the [`engine`](./engine.ts) wrapper, and sends
 * `webgpu-token` / `webgpu-end` / `webgpu-error` frames back over the same
 * WebSocket via [`sendWsFrame`](../ipc.ts).
 *
 * Only ever active in web mode — Tauri imports return immediately because
 * the underlying event listener is a no-op there.
 */

import { IS_TAURI, onWebGpuGenerate, sendWsFrame, type WebGpuGeneratePayload } from '../ipc';
import { generate, isWebGpuAvailable } from './engine';

let unlisten: (() => void) | null = null;

/**
 * Starts the bridge. Idempotent — subsequent calls are no-ops while a
 * subscription is already active.
 */
export async function startWebGpuBridge(): Promise<void> {
	if (IS_TAURI || unlisten) return;
	unlisten = await onWebGpuGenerate(handleGenerate);
}

/** Stops the bridge and tears down the WebSocket subscription. */
export function stopWebGpuBridge(): void {
	if (unlisten) {
		unlisten();
		unlisten = null;
	}
}

async function handleGenerate(req: WebGpuGeneratePayload): Promise<void> {
	if (!isWebGpuAvailable()) {
		sendWsFrame('webgpu-error', {
			request_id: req.request_id,
			message:
				"Your browser doesn't support WebGPU. Try Chrome 113+, Edge, or Safari Technology Preview, or pick a different provider."
		});
		return;
	}

	try {
		const onToken = req.streaming
			? (delta: string) => {
					if (delta.length === 0) return;
					sendWsFrame('webgpu-token', {
						request_id: req.request_id,
						delta
					});
				}
			: undefined;

		const fullText = await generate({
			modelId: req.model,
			prompt: req.prompt,
			system: req.system,
			maxTokens: req.max_tokens,
			temperature: req.temperature,
			onToken
		});

		sendWsFrame('webgpu-end', {
			request_id: req.request_id,
			full_text: fullText
		});
	} catch (err) {
		const message = err instanceof Error ? err.message : String(err);
		console.warn('WebGPU generation failed', err);
		sendWsFrame('webgpu-error', {
			request_id: req.request_id,
			message
		});
	}
}
