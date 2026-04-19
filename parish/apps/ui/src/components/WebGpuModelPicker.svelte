<script lang="ts">
	import { webGpuProvider, WEBGPU_MODELS, isWebGpuSupported, type LoadProgress } from '$lib/webgpu-provider';

	const { currentModelId = '' }: { currentModelId?: string } = $props();

	let selectedId = $state(currentModelId || WEBGPU_MODELS[0].id);
	let customId = $state('');
	let useCustom = $state(false);
	let loadProgress = $state<LoadProgress | null>(null);
	let loadError = $state<string | null>(null);
	let loadedId = $state<string | null>(webGpuProvider.currentModelId);

	const effectiveId = $derived(useCustom ? customId.trim() : selectedId);
	const isLoaded = $derived(loadedId !== null && loadedId === effectiveId);
	const isLoading = $derived(webGpuProvider.isLoading);

	const gpuSupported = isWebGpuSupported();

	async function loadModel() {
		loadError = null;
		loadProgress = { progress: 0, text: 'Initialising…' };
		try {
			await webGpuProvider.loadModel(effectiveId, (p) => {
				loadProgress = p;
			});
			loadedId = webGpuProvider.currentModelId;
			loadProgress = null;
		} catch (e) {
			loadError = e instanceof Error ? e.message : String(e);
			loadProgress = null;
		}
	}

	async function unloadModel() {
		await webGpuProvider.unload();
		loadedId = null;
	}
</script>

<div class="webgpu-picker">
	{#if !gpuSupported}
		<div class="webgpu-warn">
			⚠ WebGPU is not available in this browser. Try Chrome 113+ or Edge 113+.
		</div>
	{:else}
		<div class="webgpu-row">
			<label class="webgpu-label">Model</label>
			{#if useCustom}
				<input
					class="webgpu-input"
					type="text"
					placeholder="e.g. gemma-2-2b-it-q4f16_1-MLC"
					bind:value={customId}
					disabled={isLoading}
				/>
			{:else}
				<select class="webgpu-select" bind:value={selectedId} disabled={isLoading}>
					{#each WEBGPU_MODELS as m}
						<option value={m.id}>{m.label}</option>
					{/each}
				</select>
			{/if}
			<label class="webgpu-custom-toggle">
				<input type="checkbox" bind:checked={useCustom} disabled={isLoading} />
				custom
			</label>
		</div>

		<div class="webgpu-row">
			{#if isLoaded}
				<span class="webgpu-status ok">● Loaded: {loadedId}</span>
				<button class="webgpu-btn secondary" onclick={unloadModel} disabled={isLoading}>
					Unload
				</button>
			{:else if isLoading}
				<div class="webgpu-progress-wrap">
					<progress class="webgpu-progress" value={loadProgress?.progress ?? 0} max={1}></progress>
					<span class="webgpu-progress-text muted">{loadProgress?.text ?? 'Loading…'}</span>
				</div>
			{:else}
				<span class="webgpu-status idle">○ Not loaded</span>
				<button
					class="webgpu-btn primary"
					onclick={loadModel}
					disabled={!effectiveId}
				>
					Load model
				</button>
			{/if}
		</div>

		{#if loadError}
			<div class="webgpu-error">{loadError}</div>
		{/if}

		<div class="webgpu-hint muted">
			Models download from Hugging Face on first use and are cached in the browser.
		</div>
	{/if}
</div>

<style>
	.webgpu-picker {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 6px 0;
	}

	.webgpu-row {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.webgpu-label {
		font-size: 0.75rem;
		color: var(--color-muted, #888);
		min-width: 3rem;
	}

	.webgpu-select,
	.webgpu-input {
		flex: 1;
		min-width: 0;
		font-size: 0.75rem;
		background: var(--color-bg-subtle, #1a1a1a);
		color: var(--color-text, #eee);
		border: 1px solid var(--color-border, #333);
		border-radius: 3px;
		padding: 2px 4px;
	}

	.webgpu-custom-toggle {
		font-size: 0.7rem;
		color: var(--color-muted, #888);
		display: flex;
		align-items: center;
		gap: 3px;
		cursor: pointer;
		white-space: nowrap;
	}

	.webgpu-btn {
		font-size: 0.72rem;
		padding: 2px 8px;
		border-radius: 3px;
		border: 1px solid var(--color-border, #444);
		cursor: pointer;
	}

	.webgpu-btn.primary {
		background: var(--color-accent, #3a7bd5);
		color: #fff;
		border-color: var(--color-accent, #3a7bd5);
	}

	.webgpu-btn.secondary {
		background: transparent;
		color: var(--color-muted, #888);
	}

	.webgpu-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.webgpu-status {
		font-size: 0.72rem;
		flex: 1;
	}

	.webgpu-status.ok {
		color: var(--color-ok, #4caf50);
	}

	.webgpu-status.idle {
		color: var(--color-muted, #888);
	}

	.webgpu-progress-wrap {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.webgpu-progress {
		width: 100%;
		height: 6px;
	}

	.webgpu-progress-text {
		font-size: 0.68rem;
	}

	.webgpu-error {
		font-size: 0.72rem;
		color: var(--color-error, #e74c3c);
		word-break: break-word;
	}

	.webgpu-warn {
		font-size: 0.72rem;
		color: var(--color-warn, #e67e22);
	}

	.webgpu-hint {
		font-size: 0.68rem;
	}

	.muted {
		color: var(--color-muted, #888);
	}
</style>
