/**
 * Lazy wrapper around `@huggingface/transformers` running on WebGPU.
 *
 * Resolves a single text-generation pipeline per `modelId`. The first call
 * triggers a one-time download of the ONNX weights into the browser's
 * Cache API; subsequent loads in the same browser are offline.
 *
 * Progress for the download/initialization phase is published on a writable
 * Svelte store so a dedicated overlay component can render the bar without
 * coupling to the engine's internals.
 */

import { writable, get } from 'svelte/store';
import { detectGpuTier, findModel, type WebGpuModel } from './models';

const MODEL_STORAGE_KEY = 'parish.webgpu-model';

/**
 * Subscribable progress state for the loading overlay. `null` means
 * "no load currently in flight" — the overlay should hide itself.
 */
export interface WebGpuLoadProgress {
	model: WebGpuModel;
	reason: string;
	warning: string | null;
	/** Bytes downloaded across all weight shards. */
	loadedBytes: number;
	/** Total bytes across all weight shards (estimated until headers arrive). */
	totalBytes: number;
	/** 0…1 fraction. */
	progress: number;
	/** Phase label shown in the overlay subtitle. */
	phase: 'detecting' | 'downloading' | 'initializing' | 'ready' | 'error';
	/** Human-readable error message, if `phase === 'error'`. */
	error: string | null;
}

export const loadProgress = writable<WebGpuLoadProgress | null>(null);

interface PipelineHandle {
	modelId: string;
	// `transformers.js` types don't export the pipeline shape cleanly, so
	// we keep this loose. The real call signature is documented in their
	// README under `text-generation`.
	pipeline: (
		messages: Array<{ role: string; content: string }>,
		options: Record<string, unknown>
	) => Promise<Array<{ generated_text: Array<{ role: string; content: string }> | string }>>;
	tokenizer: { apply_chat_template?: (messages: unknown, options: unknown) => string } | null;
}

let activeHandle: Promise<PipelineHandle> | null = null;
let activeModelId: string | null = null;

/**
 * Returns the user's persisted model override, or `null` if they haven't
 * pinned one. Auto-detection is deferred to the engine when this is null.
 */
export function getStoredModelId(): string | null {
	if (typeof localStorage === 'undefined') return null;
	const stored = localStorage.getItem(MODEL_STORAGE_KEY);
	return stored && stored.length > 0 ? stored : null;
}

/**
 * Heuristic: does `id` look like a valid Hugging Face repo id (`org/name`)?
 *
 * Used to filter out the server's `model_name` default (`qwen3:14b` in a
 * fresh `GameConfig` — a perfectly good Ollama tag, but utterly wrong for
 * `transformers.js`) so that selecting `/provider webgpu` on a brand-new
 * session falls back to GPU-tier auto-detect rather than trying to load a
 * non-existent HF repo.
 */
export function isLikelyHfRepoId(id: string | null | undefined): boolean {
	if (!id) return false;
	const trimmed = id.trim();
	// Must be `org/name` with no colons (Ollama-style tags like
	// `qwen3:14b` carry a colon and are rejected on purpose).
	return /^[^\s/:]+\/[^\s/:]+$/.test(trimmed);
}

/**
 * Pure resolver that decides which model id the engine should load.
 *
 * Priority order — locked down so the precedence stays explicit:
 * 1. The player's localStorage override (`/webgpu-model <id>` or the
 *    "change model" link in the overlay) — the user has opted in, so it
 *    always wins.
 * 2. The server-passed `requestedModelId`, but only if it looks like a
 *    real HF repo id (per [`isLikelyHfRepoId`]). This protects against
 *    the server pinning the WebGPU model to its non-WebGPU default.
 * 3. Otherwise, request GPU-tier auto-detection.
 */
export type ModelChoice =
	| { kind: 'fixed'; id: string }
	| { kind: 'detect' };

export function resolveModelChoice(
	requestedModelId: string | null | undefined,
	storedModelId: string | null
): ModelChoice {
	if (storedModelId) return { kind: 'fixed', id: storedModelId };
	if (isLikelyHfRepoId(requestedModelId)) {
		return { kind: 'fixed', id: requestedModelId!.trim() };
	}
	return { kind: 'detect' };
}

/** Persists the user's chosen model so subsequent visits skip auto-detect. */
export function setStoredModelId(id: string): void {
	if (typeof localStorage === 'undefined') return;
	localStorage.setItem(MODEL_STORAGE_KEY, id);
}

/** Clears the stored override, returning the engine to auto-detect. */
export function clearStoredModelId(): void {
	if (typeof localStorage === 'undefined') return;
	localStorage.removeItem(MODEL_STORAGE_KEY);
}

/**
 * Returns the engine handle for `requestedModelId`, loading it on the first
 * call. Concurrent calls share the same in-flight load. Resolution order
 * (locked down by [`resolveModelChoice`]): player's localStorage pick,
 * then server-passed id (only if it looks like an HF repo), else GPU-tier
 * auto-detect.
 */
export async function getEngine(requestedModelId?: string | null): Promise<PipelineHandle> {
	const choice = resolveModelChoice(requestedModelId, getStoredModelId());
	const effectiveId = choice.kind === 'fixed' ? choice.id : await pickModelByTier();

	if (activeHandle && activeModelId === effectiveId) {
		return activeHandle;
	}

	// Different model — drop the previous handle and start fresh.
	activeModelId = effectiveId;
	activeHandle = loadEngine(effectiveId);
	return activeHandle;
}

async function pickModelByTier(): Promise<string> {
	const tier = await detectGpuTier();
	loadProgress.set({
		model: tier.model,
		reason: tier.reason,
		warning: tier.warning,
		loadedBytes: 0,
		totalBytes: 0,
		progress: 0,
		phase: 'detecting',
		error: null
	});
	return tier.model.id;
}

async function loadEngine(modelId: string): Promise<PipelineHandle> {
	const model =
		findModel(modelId) ??
		({
			id: modelId,
			displayName: modelId.split('/').pop() ?? modelId,
			approxSizeMb: 0,
			minMaxStorageBufferBindingSize: 0,
			minDeviceMemoryGb: 0
		} satisfies WebGpuModel);

	const current = get(loadProgress);
	loadProgress.set({
		model,
		reason: current?.reason ?? 'Loading',
		warning: current?.warning ?? null,
		loadedBytes: 0,
		totalBytes: 0,
		progress: 0,
		phase: 'downloading',
		error: null
	});

	// Per-file progress aggregation: transformers.js emits events per
	// weight shard. We keep a map of {file → {loaded, total}} and recompute
	// the overall percentage on every update.
	const fileProgress = new Map<string, { loaded: number; total: number }>();
	const updateAggregate = () => {
		let loaded = 0;
		let total = 0;
		for (const f of fileProgress.values()) {
			loaded += f.loaded;
			total += f.total;
		}
		const progress = total > 0 ? loaded / total : 0;
		loadProgress.update((p) =>
			p
				? {
						...p,
						loadedBytes: loaded,
						totalBytes: total,
						progress
					}
				: p
		);
	};

	try {
		// Dynamic import keeps `@huggingface/transformers` (≈3 MB JS bundle)
		// out of the Tauri build and lets the web bundler tree-shake it
		// when the WebGPU provider is never selected.
		const transformersMod = await import('@huggingface/transformers').catch((err) => {
			throw new Error(
				`Failed to load @huggingface/transformers — make sure dependencies are installed: ${err.message}`
			);
		});
		const { pipeline } = transformersMod as unknown as {
			pipeline: (
				task: string,
				model: string,
				options: Record<string, unknown>
			) => Promise<unknown>;
		};

		const pipe = (await pipeline('text-generation', modelId, {
			device: 'webgpu',
			dtype: 'q4',
			progress_callback: (event: {
				status: string;
				file?: string;
				loaded?: number;
				total?: number;
			}) => {
				if (event.status === 'progress' && event.file && typeof event.total === 'number') {
					fileProgress.set(event.file, {
						loaded: event.loaded ?? 0,
						total: event.total
					});
					updateAggregate();
				} else if (event.status === 'done' && event.file) {
					const entry = fileProgress.get(event.file);
					if (entry) {
						entry.loaded = entry.total;
						fileProgress.set(event.file, entry);
						updateAggregate();
					}
				}
			}
		})) as PipelineHandle['pipeline'] & {
			tokenizer?: PipelineHandle['tokenizer'];
		};

		loadProgress.update((p) =>
			p ? { ...p, phase: 'initializing', progress: 1, loadedBytes: p.totalBytes } : p
		);

		const handle: PipelineHandle = {
			modelId,
			pipeline: pipe as unknown as PipelineHandle['pipeline'],
			tokenizer: (pipe as { tokenizer?: PipelineHandle['tokenizer'] }).tokenizer ?? null
		};

		loadProgress.update((p) => (p ? { ...p, phase: 'ready' } : p));
		// Keep the overlay around briefly so the user sees "Ready" before
		// it auto-dismisses.
		setTimeout(() => loadProgress.set(null), 1200);
		return handle;
	} catch (err) {
		const message = err instanceof Error ? err.message : String(err);
		loadProgress.update((p) =>
			p ? { ...p, phase: 'error', error: message } : null
		);
		// Reset the cached handle so a retry can re-attempt.
		activeHandle = null;
		activeModelId = null;
		throw err;
	}
}

/**
 * Runs a single generation. Streams tokens via `onToken` if provided, and
 * always returns the full assembled text.
 */
export async function generate(opts: {
	modelId: string;
	prompt: string;
	system: string | null;
	maxTokens: number | null;
	temperature: number | null;
	onToken?: (delta: string) => void;
}): Promise<string> {
	const handle = await getEngine(opts.modelId);

	const messages = [
		...(opts.system ? [{ role: 'system', content: opts.system }] : []),
		{ role: 'user', content: opts.prompt }
	];

	// Build a TextStreamer if streaming was requested. The streamer is a
	// runtime export from `@huggingface/transformers`; we import lazily so
	// non-streaming calls don't pay the cost.
	let streamer: unknown = undefined;
	if (opts.onToken) {
		const transformersMod = (await import('@huggingface/transformers')) as unknown as {
			TextStreamer: new (
				tokenizer: unknown,
				options: { skip_prompt: boolean; callback_function: (text: string) => void }
			) => unknown;
		};
		streamer = new transformersMod.TextStreamer(handle.tokenizer, {
			skip_prompt: true,
			callback_function: (text: string) => {
				try {
					opts.onToken!(text);
				} catch (e) {
					console.warn('WebGPU onToken callback threw', e);
				}
			}
		});
	}

	const result = await handle.pipeline(messages, {
		max_new_tokens: opts.maxTokens ?? 512,
		temperature: opts.temperature ?? 0.7,
		do_sample: opts.temperature ? opts.temperature > 0 : true,
		streamer
	});

	// `transformers.js` returns either an array of {generated_text:
	// messages-array} (chat template) or {generated_text: string}. Pull out
	// the assistant's reply.
	const first = result?.[0]?.generated_text;
	if (Array.isArray(first)) {
		// Chat-template path: last message is the assistant turn.
		const last = first[first.length - 1];
		return typeof last?.content === 'string' ? last.content : '';
	}
	return typeof first === 'string' ? first : '';
}

/** True if the runtime advertises WebGPU support. */
export function isWebGpuAvailable(): boolean {
	return typeof navigator !== 'undefined' && 'gpu' in navigator;
}
