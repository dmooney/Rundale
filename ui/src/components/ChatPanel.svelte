<script lang="ts">
	import { tick } from 'svelte';
	import { textLog, streamingActive, loadingSpinner, loadingPhrase, loadingColor } from '../stores/game';
	import type { TextLogEntry } from '$lib/types';

	let logEl: HTMLDivElement;

	$effect(() => {
		// Scroll to bottom when log changes
		const _ = $textLog;
		tick().then(() => {
			if (logEl) {
				logEl.scrollTop = logEl.scrollHeight;
			}
		});
	});

	function entryClass(entry: TextLogEntry): string {
		if (entry.source === 'player') return 'entry player';
		if (entry.source === 'system') return 'entry system';
		return 'entry npc';
	}
</script>

<div class="chat-panel" bind:this={logEl}>
	{#each $textLog as entry (entry)}
		<div class={entryClass(entry)}>
			{#if entry.source !== 'system'}
				<span class="source">{entry.source === 'player' ? 'You' : entry.source}:</span>
			{/if}
			<span class="content">{entry.content}{#if entry.streaming}<span class="cursor">▋</span>{/if}</span>
		</div>
	{/each}
	{#if $streamingActive && ($textLog.length === 0 || !$textLog[$textLog.length - 1].streaming)}
		<div class="loading-row">
			<span class="loading-spinner" style="color: rgb({$loadingColor[0]}, {$loadingColor[1]}, {$loadingColor[2]})">{$loadingSpinner}</span>
			<span class="loading-phrase" style="color: rgb({$loadingColor[0]}, {$loadingColor[1]}, {$loadingColor[2]})">{$loadingPhrase}</span>
		</div>
	{/if}
</div>

<style>
	.chat-panel {
		flex: 1;
		overflow-y: auto;
		padding: 1rem;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
		background: var(--color-bg);
	}

	.entry {
		line-height: 1.6;
		font-size: 1.15rem;
		white-space: pre-wrap;
	}

	.source {
		font-weight: 600;
		margin-right: 0.5rem;
	}

	.player .source {
		color: var(--color-muted);
	}

	.npc .source {
		color: var(--color-accent);
	}

	.system .content {
		color: var(--color-fg);
	}

	.cursor {
		display: inline-block;
		animation: blink 1s step-end infinite;
	}

	@keyframes blink {
		0%, 100% { opacity: 1; }
		50% { opacity: 0; }
	}

	.loading-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0;
		font-size: 1.1rem;
		animation: fade-in 0.3s ease-in;
	}

	.loading-spinner {
		display: inline-block;
		font-size: 1.3rem;
		animation: pulse 1.2s ease-in-out infinite;
	}

	.loading-phrase {
		font-style: italic;
		transition: color 0.5s ease;
	}

	@keyframes fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; transform: scale(1); }
		50% { opacity: 0.6; transform: scale(1.15); }
	}
</style>
