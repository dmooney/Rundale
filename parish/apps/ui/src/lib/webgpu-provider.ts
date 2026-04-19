/**
 * WebGPU inference provider — runs LLM models locally in the browser via WebLLM.
 *
 * Models are downloaded from Hugging Face on first use and cached in the
 * browser's Cache API. Subsequent sessions reuse the cached weights.
 *
 * Only valid in the web client (not Tauri). Check `isWebGpuSupported()` before
 * showing any WebGPU-related UI.
 */

import * as webllm from '@mlc-ai/web-llm';

// ── Model catalogue ──────────────────────────────────────────────────────────

export interface WebGpuModel {
	id: string;
	label: string;
	/** Approximate download size shown to the user. */
	size: string;
}

/**
 * Curated Gemma models known to work well with WebGPU via WebLLM.
 * The `id` values must match entries in `webllm.prebuiltAppConfig.model_list`.
 *
 * Note: Gemma 4 model IDs will appear here once web-llm publishes support.
 * Users can also enter any valid WebLLM model ID via the custom field.
 */
export const WEBGPU_MODELS: WebGpuModel[] = [
	{
		id: 'gemma-2-2b-it-q4f16_1-MLC',
		label: 'Gemma 2 2B (fast, ~1.4 GB)',
		size: '~1.4 GB'
	},
	{
		id: 'gemma-2-2b-it-q4f32_1-MLC',
		label: 'Gemma 2 2B q4f32 (~1.5 GB)',
		size: '~1.5 GB'
	},
	{
		id: 'gemma-2-9b-it-q4f16_1-MLC',
		label: 'Gemma 2 9B (~5.5 GB)',
		size: '~5.5 GB'
	},
	{
		id: 'gemma-2-9b-it-q4f32_1-MLC',
		label: 'Gemma 2 9B q4f32 (~5.6 GB)',
		size: '~5.6 GB'
	}
];

// ── WebGPU availability ──────────────────────────────────────────────────────

/** Returns true if the browser supports WebGPU. */
export function isWebGpuSupported(): boolean {
	return typeof navigator !== 'undefined' && 'gpu' in navigator;
}

// ── Provider singleton ────────────────────────────────────────────────────────

export type LoadProgress = { progress: number; text: string };
export type LoadProgressCallback = (p: LoadProgress) => void;

class WebGpuProvider {
	private engine: webllm.MLCEngine | null = null;
	private loadedModelId: string | null = null;
	private loading = false;

	get isLoaded(): boolean {
		return this.engine !== null && this.loadedModelId !== null;
	}

	get currentModelId(): string | null {
		return this.loadedModelId;
	}

	get isLoading(): boolean {
		return this.loading;
	}

	/**
	 * Loads (or switches to) a model. Safe to call while another load is in
	 * progress — subsequent calls will resolve after the current one settles.
	 */
	async loadModel(modelId: string, onProgress?: LoadProgressCallback): Promise<void> {
		if (this.loadedModelId === modelId && this.engine) return;

		this.loading = true;
		try {
			// Unload the existing engine before allocating a new one so the old
			// and new model weights don't coexist in GPU memory during the load.
			// Without this, switching models requires old+new model size available.
			if (this.engine) {
				await this.engine.unload();
				this.engine = null;
				this.loadedModelId = null;
			}
			const engine = new webllm.MLCEngine();
			engine.setInitProgressCallback((report: webllm.InitProgressReport) => {
				onProgress?.({ progress: report.progress, text: report.text });
			});
			await engine.reload(modelId);
			this.engine = engine;
			this.loadedModelId = modelId;
		} finally {
			this.loading = false;
		}
	}

	/**
	 * Runs inference and calls `onToken` for each streamed token.
	 * Returns the full assembled response text.
	 */
	async generate(
		prompt: string,
		system: string | null,
		maxTokens: number | null,
		temperature: number | null,
		onToken: (token: string) => void
	): Promise<string> {
		if (!this.engine) {
			throw new Error('WebGPU model not loaded — call loadModel() first');
		}

		const messages: webllm.ChatCompletionMessageParam[] = [];
		if (system) {
			messages.push({ role: 'system', content: system });
		}
		messages.push({ role: 'user', content: prompt });

		const chunks = await this.engine.chat.completions.create({
			messages,
			stream: true,
			max_tokens: maxTokens ?? undefined,
			temperature: temperature ?? undefined
		});

		let fullText = '';
		for await (const chunk of chunks) {
			const delta = chunk.choices[0]?.delta?.content ?? '';
			if (delta) {
				onToken(delta);
				fullText += delta;
			}
		}
		return fullText;
	}

	/** Unloads the current model and frees GPU memory. */
	async unload(): Promise<void> {
		if (this.engine) {
			await this.engine.unload();
			this.engine = null;
			this.loadedModelId = null;
		}
	}
}

export const webGpuProvider = new WebGpuProvider();
