<script lang="ts">
	import type { SaveFileInfo, SaveState } from '$lib/types';

	let {
		files = [] as SaveFileInfo[],
		saveState = null as SaveState | null,
		loading = false,
		onswitchledger = (_file: SaveFileInfo) => {},
		onforkledger = () => {},
		onnewgame = () => {}
	} = $props();
</script>

<style>
	.ledger-row {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		padding: 0.35rem 0.5rem;
		font-size: 0.8rem;
		border-bottom: 1px solid var(--color-border);
	}
	.ledger-row:last-child {
		border-bottom: none;
	}
	.ledger-row:hover {
		background: var(--color-input-bg);
	}
	.ledger-row.ledger-active {
		background: var(--color-input-bg);
	}

	.file-number {
		color: var(--color-muted);
		font-size: 0.8rem;
		flex-shrink: 0;
	}

	.file-name {
		color: var(--color-accent);
		font-size: 0.85rem;
		flex-shrink: 0;
	}

	.ledger-meta {
		color: var(--color-muted);
		font-size: 0.75rem;
		flex: 1;
	}

	.ledger-current {
		font-size: 0.6rem;
		color: var(--color-muted);
		font-style: italic;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.ledger-locked {
		font-size: 0.6rem;
		color: var(--color-muted);
		font-style: italic;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		opacity: 0.6;
	}

	.new-ledger {
		border-bottom: none;
		cursor: pointer;
	}
	.new-ledger:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: -2px;
	}
	.new-ledger[aria-disabled='true'] {
		opacity: 0.5;
		cursor: default;
	}

	.action-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.6rem;
		padding: 0.15rem 0.4rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.action-btn:hover:not(:disabled),
	.action-btn:focus-visible:not(:disabled) {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}
	.action-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>

{#each files as file, fileIdx (file.filename)}
	{@const isActive = file.filename === saveState?.filename}
	<div class="ledger-row" class:ledger-active={isActive}>
		<span class="file-number">{fileIdx + 1}.</span>
		<span class="file-name">{file.filename}</span>
		<span class="ledger-meta">
			{file.file_size}
			{#if file.branches[0]?.latest_location}
				— {file.branches[0].latest_location}
			{/if}
		</span>
		{#if isActive}
			<span class="ledger-current">You are here</span>
		{:else if file.locked}
			<span class="ledger-locked">In Use</span>
		{:else}
			<button class="action-btn" onclick={() => onswitchledger(file)} disabled={loading}>Open</button>
		{/if}
	</div>
{/each}

<div class="ledger-row new-ledger" onclick={() => { if (!loading) onforkledger(); }} role="button" tabindex="0" aria-disabled={loading} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (!loading && !e.repeat) onforkledger(); } }}>
	<span class="file-number">+</span>
	<span class="file-name">Fork New Ledger</span>
</div>

<div class="ledger-row new-ledger" onclick={() => { if (!loading) onnewgame(); }} role="button" tabindex="0" aria-disabled={loading} onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (!loading && !e.repeat) onnewgame(); } }}>
	<span class="file-number">+</span>
	<span class="file-name">New Game</span>
</div>
