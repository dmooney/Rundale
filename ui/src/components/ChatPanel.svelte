<script lang="ts">
	import { tick } from 'svelte';
	import { textLog, streamingActive } from '../stores/game';
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
		<div class="spinner-row">
			<span class="spinner"></span>
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

	.spinner-row {
		display: flex;
		align-items: center;
		padding: 0.25rem 0;
	}

	.spinner {
		display: inline-block;
		width: 1rem;
		height: 1rem;
		border: 2px solid var(--color-border);
		border-top-color: var(--color-accent);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
