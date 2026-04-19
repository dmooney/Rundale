/**
 * WebGPU model registry + GPU-tier auto-detection.
 *
 * WebGPU deliberately does *not* expose total VRAM (privacy / fingerprinting
 * concerns), so we use `GPUAdapter.limits.maxStorageBufferBindingSize` as a
 * proxy — on real devices it scales roughly with available VRAM. We also
 * read `navigator.deviceMemory` (system RAM in GB) as a secondary signal
 * because integrated GPUs share it with the rest of the system.
 *
 * The user can always override the auto-selected model with
 * `/model <hf-repo-id>` (persisted under `parish.webgpu-model` in
 * localStorage).
 */

/** A single Hugging Face model entry that the bridge can run. */
export interface WebGpuModel {
	/** Hugging Face repo id (e.g. `onnx-community/gemma-4-E2B-it-ONNX`). */
	id: string;
	/** Human-readable name shown in the loading overlay. */
	displayName: string;
	/** Approximate on-disk size after quantisation, in MB. */
	approxSizeMb: number;
	/** Minimum `maxStorageBufferBindingSize` (bytes) required to run. */
	minMaxStorageBufferBindingSize: number;
	/** Minimum `navigator.deviceMemory` (GB) recommended. `0` = no requirement. */
	minDeviceMemoryGb: number;
}

const GIB = 1024 * 1024 * 1024;

/**
 * Tier table — ordered from most-capable to least-capable. The first entry
 * whose requirements are satisfied is auto-selected.
 *
 * The default-tier (Gemma 4 E2B ~1.5 GB) matches the linked
 * `webml-community/Gemma-4-WebGPU` demo so first-time loads are reasonably
 * sized while still high-quality.
 */
export const WEBGPU_MODELS: readonly WebGpuModel[] = [
	{
		id: 'onnx-community/gemma-4-E4B-it-ONNX',
		displayName: 'Gemma 4 E4B (~3 GB)',
		approxSizeMb: 3072,
		minMaxStorageBufferBindingSize: 4 * GIB,
		minDeviceMemoryGb: 8
	},
	{
		id: 'onnx-community/gemma-4-E2B-it-ONNX',
		displayName: 'Gemma 4 E2B (~1.5 GB)',
		approxSizeMb: 1536,
		minMaxStorageBufferBindingSize: 2 * GIB,
		minDeviceMemoryGb: 0
	}
];

/** Fallback used when no tier matches (very low-VRAM devices). */
export const FALLBACK_MODEL: WebGpuModel = WEBGPU_MODELS[WEBGPU_MODELS.length - 1];

export interface GpuTierResult {
	model: WebGpuModel;
	/** Short, user-facing reason explaining why this model was picked. */
	reason: string;
	/** Set when WebGPU is missing or no tier comfortably matches. */
	warning: string | null;
}

/**
 * Probes the browser's WebGPU adapter and returns the best-fit model.
 *
 * Throws nothing — on any error returns `FALLBACK_MODEL` with a warning.
 */
export async function detectGpuTier(): Promise<GpuTierResult> {
	if (typeof navigator === 'undefined' || !('gpu' in navigator)) {
		return {
			model: FALLBACK_MODEL,
			reason: 'WebGPU not available in this browser',
			warning:
				"Your browser doesn't support WebGPU. Try Chrome 113+, Edge, or Safari Technology Preview, or pick a different provider."
		};
	}

	let adapter: GPUAdapter | null = null;
	try {
		adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
	} catch (err) {
		console.warn('WebGPU adapter request failed', err);
	}

	if (!adapter) {
		return {
			model: FALLBACK_MODEL,
			reason: 'No WebGPU adapter could be acquired',
			warning:
				'Your browser advertises WebGPU but no adapter responded. The model may fail to load.'
		};
	}

	const maxBuf = adapter.limits.maxStorageBufferBindingSize;
	// `navigator.deviceMemory` is implemented in Chromium-family browsers
	// and reports system RAM in GB (capped at 8). Treat undefined as 0.
	const navigatorWithMemory = navigator as Navigator & { deviceMemory?: number };
	const deviceMemoryGb = navigatorWithMemory.deviceMemory ?? 0;

	for (const model of WEBGPU_MODELS) {
		if (
			maxBuf >= model.minMaxStorageBufferBindingSize &&
			deviceMemoryGb >= model.minDeviceMemoryGb
		) {
			return {
				model,
				reason: `Auto-selected for your GPU (max storage buffer ${formatBytes(maxBuf)}${
					deviceMemoryGb > 0 ? `, ${deviceMemoryGb} GB RAM` : ''
				})`,
				warning: null
			};
		}
	}

	return {
		model: FALLBACK_MODEL,
		reason: `GPU reports only ${formatBytes(maxBuf)} max storage buffer — the smallest tier may still OOM`,
		warning:
			"Your GPU is below the recommended size for any of the bundled models. The download will proceed but the tab may crash if the model can't fit."
	};
}

function formatBytes(bytes: number): string {
	const gb = bytes / GIB;
	if (gb >= 1) return `${gb.toFixed(1)} GB`;
	const mb = bytes / (1024 * 1024);
	return `${mb.toFixed(0)} MB`;
}

/** Returns the model entry matching `id`, or `null` if unknown. */
export function findModel(id: string): WebGpuModel | null {
	return WEBGPU_MODELS.find((m) => m.id === id) ?? null;
}
