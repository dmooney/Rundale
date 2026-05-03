<script lang="ts">
	import { demoEnabled, demoPaused, demoTurnCount, demoStatus, demoConfig, demoVisible } from '../stores/demo';
	import { startDemoLoop, stopDemo } from '../lib/demo-player';

	let localPause = $state(2);
	let localMaxTurns = $state(0);
	let localPrompt = $state('');

	// Sync local fields from store on mount
	$effect(() => {
		const cfg = $demoConfig;
		localPause = cfg.turn_pause_secs;
		localMaxTurns = cfg.max_turns ?? 0;
		localPrompt = cfg.extra_prompt ?? '';
	});

	function applyAndStart() {
		demoConfig.set({
			auto_start: false,
			extra_prompt: localPrompt.trim() || null,
			turn_pause_secs: Math.max(0, localPause),
			max_turns: localMaxTurns > 0 ? localMaxTurns : null
		});
		startDemoLoop();
	}

	function close() {
		demoVisible.set(false);
	}
</script>

<div class="demo-panel" role="dialog" aria-label="Demo mode configuration">
	<div class="demo-panel-header">
		<span>DEMO MODE</span>
		<button type="button" class="close-btn" aria-label="Close demo panel" onclick={close}>✕</button>
	</div>

	<div class="demo-panel-body">
		<label class="field-row">
			<span class="field-label">Pause between turns (s)</span>
			<input
				type="number"
				min="0"
				max="30"
				step="0.5"
				bind:value={localPause}
				class="field-input"
			/>
		</label>

		<label class="field-row">
			<span class="field-label">Max turns (0 = unlimited)</span>
			<input
				type="number"
				min="0"
				step="1"
				bind:value={localMaxTurns}
				class="field-input"
			/>
		</label>

		<div class="field-row field-col">
			<span class="field-label">Extra prompt instructions</span>
			<textarea
				bind:value={localPrompt}
				class="field-textarea"
				rows="4"
				placeholder="Optional: extra instructions for the LLM player..."
			></textarea>
		</div>

		<div class="demo-status-row">
			<span class="status-label">Turn:</span>
			<span>{$demoTurnCount}</span>
			<span class="status-label">Status:</span>
			<span>{$demoStatus}</span>
		</div>

		<div class="demo-actions">
			{#if $demoEnabled}
				<button type="button" class="action-btn" onclick={() => demoPaused.update((v) => !v)}>
					{$demoPaused ? 'Resume' : 'Pause'}
				</button>
				<button type="button" class="action-btn action-stop" onclick={stopDemo}>Stop</button>
			{:else}
				<button type="button" class="action-btn action-start" onclick={applyAndStart}>
					Apply &amp; Start
				</button>
			{/if}
		</div>
	</div>
</div>

<style>
	.demo-panel {
		position: fixed;
		bottom: 0;
		right: 0;
		width: 22rem;
		max-height: 80vh;
		overflow-y: auto;
		background: var(--color-panel-bg, #1a1a2e);
		border: 1px solid var(--color-border, #444);
		border-bottom: none;
		border-right: none;
		z-index: 150;
		font-family: var(--font-display, monospace);
		font-size: 0.72rem;
		color: var(--color-fg, #ccc);
	}

	.demo-panel-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.4rem 0.75rem;
		border-bottom: 1px solid var(--color-border, #444);
		letter-spacing: 0.1em;
		color: var(--color-accent, #b08531);
		font-weight: bold;
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-muted, #888);
		cursor: pointer;
		font-size: 0.8rem;
		padding: 0;
		line-height: 1;
	}

	.close-btn:hover {
		color: var(--color-fg, #ccc);
	}

	.demo-panel-body {
		padding: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	.field-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.field-col {
		flex-direction: column;
		align-items: stretch;
	}

	.field-label {
		color: var(--color-muted, #888);
		flex: 1;
		white-space: nowrap;
	}

	.field-input {
		width: 5rem;
		background: var(--color-input-bg, #111);
		border: 1px solid var(--color-border, #444);
		color: var(--color-fg, #ccc);
		font-family: inherit;
		font-size: inherit;
		padding: 0.15rem 0.3rem;
	}

	.field-textarea {
		width: 100%;
		background: var(--color-input-bg, #111);
		border: 1px solid var(--color-border, #444);
		color: var(--color-fg, #ccc);
		font-family: inherit;
		font-size: inherit;
		padding: 0.25rem 0.4rem;
		resize: vertical;
		margin-top: 0.25rem;
	}

	.demo-status-row {
		display: flex;
		gap: 0.5rem;
		align-items: center;
		color: var(--color-muted, #888);
	}

	.status-label {
		color: var(--color-muted, #888);
	}

	.demo-actions {
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.action-btn {
		background: none;
		border: 1px solid var(--color-border, #444);
		color: var(--color-fg, #ccc);
		font-family: inherit;
		font-size: inherit;
		padding: 0.2rem 0.6rem;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
	}

	.action-btn:hover,
	.action-btn:focus-visible {
		color: var(--color-accent, #b08531);
		border-color: var(--color-accent, #b08531);
	}

	.action-start {
		color: var(--color-accent, #b08531);
		border-color: var(--color-accent, #b08531);
	}

	.action-stop:hover,
	.action-stop:focus-visible {
		color: #c0392b;
		border-color: #c0392b;
	}
</style>
