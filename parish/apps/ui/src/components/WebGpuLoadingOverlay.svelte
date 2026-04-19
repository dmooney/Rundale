<script lang="ts">
	import { loadProgress, clearStoredModelId } from '$lib/webgpu/engine';
	import { WEBGPU_MODELS, findModel } from '$lib/webgpu/models';
	import { setStoredModelId } from '$lib/webgpu/engine';

	let manualPickerOpen = $state(false);

	function formatBytes(bytes: number): string {
		if (bytes <= 0) return '0 MB';
		const mb = bytes / (1024 * 1024);
		if (mb < 1024) return `${mb.toFixed(0)} MB`;
		return `${(mb / 1024).toFixed(2)} GB`;
	}

	function pickModel(id: string): void {
		setStoredModelId(id);
		manualPickerOpen = false;
		// Setting the override doesn't itself reload — the next inference
		// request will hit `getEngine` which sees the new id and starts a
		// fresh load. Show a transient hint in the overlay.
		loadProgress.update((p) =>
			p
				? {
						...p,
						reason: `Will switch to ${findModel(id)?.displayName ?? id} on next request`
					}
				: null
		);
	}

	function resetToAuto(): void {
		clearStoredModelId();
		manualPickerOpen = false;
	}

	function dismiss(): void {
		// Allow manual dismissal once we're at the ready/error phase. While
		// downloading, the overlay must stay visible so the user knows the
		// tab is busy.
		const p = $loadProgress;
		if (p && (p.phase === 'ready' || p.phase === 'error')) {
			loadProgress.set(null);
		}
	}
</script>

{#if $loadProgress}
	{@const p = $loadProgress}
	<div
		class="webgpu-overlay"
		role="dialog"
		aria-modal="true"
		aria-labelledby="webgpu-overlay-title"
	>
		<div class="webgpu-card">
			<header>
				<h2 id="webgpu-overlay-title">{p.model.displayName}</h2>
				<p class="reason">{p.reason}</p>
			</header>

			{#if p.phase === 'error'}
				<div class="error">
					<p>WebGPU model load failed:</p>
					<pre>{p.error ?? 'unknown error'}</pre>
					<p class="hint">
						Try a smaller model below, switch to a different provider with
						<code>/provider ollama</code>, or refresh the page to retry.
					</p>
				</div>
			{:else}
				<div class="progress" aria-live="polite">
					<div class="bar" aria-hidden="true">
						<div class="fill" style="width: {(p.progress * 100).toFixed(1)}%"></div>
					</div>
					<div class="numbers">
						<span class="percent">{(p.progress * 100).toFixed(0)}%</span>
						<span class="bytes">
							{formatBytes(p.loadedBytes)} / {formatBytes(p.totalBytes)}
						</span>
					</div>
					<p class="phase">
						{#if p.phase === 'detecting'}
							Detecting your GPU…
						{:else if p.phase === 'downloading'}
							Downloading model — first-time download stays cached in your browser, so this only
							happens once.
						{:else if p.phase === 'initializing'}
							Initializing on WebGPU…
						{:else if p.phase === 'ready'}
							Ready!
						{/if}
					</p>
				</div>
			{/if}

			{#if p.warning}
				<p class="warning">{p.warning}</p>
			{/if}

			<footer>
				{#if !manualPickerOpen}
					<button type="button" class="link" onclick={() => (manualPickerOpen = true)}
						>Change model</button
					>
				{:else}
					<div class="picker">
						<p>Pick a smaller or larger model:</p>
						<ul>
							{#each WEBGPU_MODELS as m (m.id)}
								<li>
									<button type="button" onclick={() => pickModel(m.id)}>
										{m.displayName}
										<span class="picker-id">{m.id}</span>
									</button>
								</li>
							{/each}
						</ul>
						<button type="button" class="link" onclick={resetToAuto}>Use auto-detect</button>
					</div>
				{/if}

				{#if p.phase === 'ready' || p.phase === 'error'}
					<button type="button" class="dismiss" onclick={dismiss}>Dismiss</button>
				{/if}
			</footer>
		</div>
	</div>
{/if}

<style>
	.webgpu-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.7);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
		padding: 1rem;
	}
	.webgpu-card {
		background: var(--panel-bg, #1d1f21);
		color: var(--fg, #f8f8f2);
		border: 1px solid var(--border, #44475a);
		border-radius: 8px;
		max-width: 520px;
		width: 100%;
		padding: 1.5rem;
		box-shadow: 0 10px 40px rgba(0, 0, 0, 0.5);
	}
	header h2 {
		margin: 0 0 0.25rem 0;
		font-size: 1.15rem;
	}
	.reason {
		margin: 0 0 1rem 0;
		font-size: 0.85rem;
		color: var(--muted, #8a8f95);
	}
	.progress {
		margin: 1rem 0;
	}
	.bar {
		height: 0.6rem;
		background: var(--input-bg, #2b2d31);
		border-radius: 999px;
		overflow: hidden;
	}
	.fill {
		height: 100%;
		background: var(--accent, #8be9fd);
		transition: width 200ms ease;
	}
	.numbers {
		display: flex;
		justify-content: space-between;
		margin-top: 0.4rem;
		font-size: 0.85rem;
	}
	.percent {
		font-weight: 600;
	}
	.bytes {
		color: var(--muted, #8a8f95);
	}
	.phase {
		margin: 0.6rem 0 0 0;
		font-size: 0.85rem;
		color: var(--muted, #8a8f95);
	}
	.warning {
		margin: 1rem 0 0 0;
		padding: 0.6rem 0.8rem;
		background: rgba(255, 184, 108, 0.15);
		border-left: 3px solid #ffb86c;
		font-size: 0.85rem;
	}
	.error {
		padding: 0.8rem 1rem;
		background: rgba(255, 85, 85, 0.15);
		border-left: 3px solid #ff5555;
		border-radius: 4px;
	}
	.error pre {
		font-size: 0.8rem;
		white-space: pre-wrap;
		word-break: break-word;
		margin: 0.4rem 0;
	}
	.hint {
		margin: 0.4rem 0 0 0;
		font-size: 0.85rem;
	}
	footer {
		margin-top: 1rem;
		display: flex;
		justify-content: space-between;
		align-items: flex-start;
		gap: 1rem;
		flex-wrap: wrap;
	}
	.link {
		background: none;
		border: none;
		color: var(--accent, #8be9fd);
		text-decoration: underline;
		cursor: pointer;
		padding: 0;
		font-size: 0.9rem;
	}
	.dismiss {
		background: var(--accent, #8be9fd);
		color: var(--bg, #1d1f21);
		border: none;
		border-radius: 4px;
		padding: 0.4rem 0.9rem;
		cursor: pointer;
		font-weight: 600;
	}
	.picker ul {
		list-style: none;
		padding: 0;
		margin: 0.4rem 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.picker li button {
		width: 100%;
		text-align: left;
		background: var(--input-bg, #2b2d31);
		color: var(--fg, #f8f8f2);
		border: 1px solid var(--border, #44475a);
		border-radius: 4px;
		padding: 0.4rem 0.6rem;
		cursor: pointer;
		font-size: 0.85rem;
	}
	.picker li button:hover {
		border-color: var(--accent, #8be9fd);
	}
	.picker-id {
		display: block;
		color: var(--muted, #8a8f95);
		font-size: 0.75rem;
	}
	.picker p {
		margin: 0 0 0.3rem 0;
		font-size: 0.85rem;
	}
	code {
		background: var(--input-bg, #2b2d31);
		padding: 0.05rem 0.3rem;
		border-radius: 3px;
	}
</style>
