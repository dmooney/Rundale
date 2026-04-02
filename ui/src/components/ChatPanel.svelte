<script lang="ts">
	import { tick } from 'svelte';
	import { textLog, streamingActive, loadingPhrase, loadingColor } from '../stores/game';
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

	function entryType(entry: TextLogEntry): 'player' | 'npc' | 'system' {
		if (entry.source === 'player') return 'player';
		if (entry.source === 'system') return 'system';
		return 'npc';
	}

	function displayLabel(entry: TextLogEntry): string {
		if (entry.source === 'player') return 'You';
		return entry.source;
	}
</script>

<div class="chat-panel" data-testid="chat-panel" bind:this={logEl}>
	{#each $textLog as entry (entry)}
		{#if entryType(entry) === 'system'}
			{@const isSplash = entry.content.includes('Copyright \u00A9')}
			{@const lines = entry.content.split('\n')}
			<div class="entry system">
				{#if isSplash}
					<span class="content"><strong>{lines[0]}</strong>{'\n' + lines.slice(1).join('\n')}</span>
				{:else}
					<span class="content">{entry.content}</span>
				{/if}
			</div>
		{:else}
			<div class="bubble-row {entryType(entry)}">
				<div class="bubble-wrapper">
					<span class="label">{displayLabel(entry)}</span>
					<div class="bubble">
						<span class="content"
							>{entry.content}{#if entry.streaming}<span class="cursor">▋</span
							>{/if}</span
						>
					</div>
				</div>
			</div>
		{/if}
	{/each}
	{#if $streamingActive && ($textLog.length === 0 || !$textLog[$textLog.length - 1].streaming)}
		<div class="loading-row">
			<svg class="triquetra-spinner" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
				<circle class="knot-circle" pathLength="120"
					cx="50" cy="50" r="16"
					fill="none" stroke="var(--color-accent)" stroke-width="3"
					stroke-linecap="round" />
				<path class="triquetra-path" pathLength="120"
					d="M 50 22
					   A 28 28 0 0 0 74.25 64
					   A 28 28 0 0 0 25.75 64
					   A 28 28 0 0 0 50 22 Z"
					fill="none" stroke="var(--color-accent)" stroke-width="3"
					stroke-linecap="round" stroke-linejoin="round" />
			</svg>
			<span class="loading-phrase" style="color: rgb({$loadingColor[0]}, {$loadingColor[1]}, {$loadingColor[2]})">{$loadingPhrase}</span>
		</div>
	{/if}
</div>

<style>
	.chat-panel {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 1rem;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		gap: 0.6rem;
		background: var(--color-bg);
	}

	/* System messages: full-width, no bubble */
	.entry.system {
		line-height: 1.6;
		font-size: 1.05rem;
		color: var(--color-fg);
		white-space: pre-wrap;
		padding: 0.25rem 0;
	}

	/* Bubble row: flex container controlling left/right alignment */
	.bubble-row {
		display: flex;
		width: 100%;
	}

	.bubble-row.npc {
		justify-content: flex-start;
	}

	.bubble-row.player {
		justify-content: flex-end;
	}

	/* Wrapper keeps label + bubble aligned together */
	.bubble-wrapper {
		display: flex;
		flex-direction: column;
		max-width: 75%;
	}

	/* Name label above the bubble */
	.label {
		font-size: 0.8rem;
		font-weight: 600;
		margin-bottom: 0.2rem;
		padding: 0 0.5rem;
	}

	.npc .label {
		color: var(--color-accent);
		text-align: left;
	}

	.player .label {
		color: var(--color-muted);
		text-align: right;
	}

	/* Message bubble */
	.bubble {
		padding: 0.6rem 0.9rem;
		border-radius: 1rem;
		font-size: 1.1rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-wrap: break-word;
	}

	.npc .bubble {
		background: var(--color-border);
		color: var(--color-fg);
		border-top-left-radius: 0.25rem;
	}

	.player .bubble {
		background: var(--color-accent);
		color: var(--color-bg);
		border-top-right-radius: 0.25rem;
	}

	.cursor {
		display: inline-block;
		animation: blink 1s step-end infinite;
	}

	@keyframes blink {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0;
		}
	}

	.loading-row {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.25rem 0;
		font-size: 1.1rem;
		animation: fade-in 0.3s ease-in;
	}

	.loading-phrase {
		font-style: italic;
		transition: color 0.5s ease;
	}

	@keyframes fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.triquetra-spinner {
		width: 2.5rem;
		height: 2.5rem;
		animation: triquetra-rotate 6s linear infinite;
	}

	.triquetra-path {
		stroke-dasharray: 80 40;
		stroke-dashoffset: 0;
		animation: triquetra-draw 2.4s linear infinite;
	}

	.knot-circle {
		stroke-dasharray: 0 120;
		stroke-dashoffset: 0;
		animation: circle-draw 3s ease-in-out infinite;
		animation-delay: 0.4s;
	}

	@keyframes triquetra-draw {
		to {
			stroke-dashoffset: -120;
		}
	}

	@keyframes circle-draw {
		0%   { stroke-dasharray: 0 120;   stroke-dashoffset: 0; }
		30%  { stroke-dasharray: 120 120; stroke-dashoffset: 0; }
		70%  { stroke-dasharray: 120 120; stroke-dashoffset: 0; }
		100% { stroke-dasharray: 0 120;   stroke-dashoffset: -120; }
	}

	@keyframes triquetra-rotate {
		to {
			transform: rotate(360deg);
		}
	}
</style>
