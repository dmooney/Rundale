<script lang="ts">
	import { npcsHere, languageHints, nameHints, uiConfig } from '../stores/game';
	import MoodIcon from './MoodIcon.svelte';
</script>

<aside class="sidebar" data-testid="sidebar">
	<details open>
		<summary>NPCs Here</summary>
		{#if $npcsHere.length > 0}
			<ul class="npc-list">
				{#each $npcsHere as npc}
					<li class="npc-item">
						<div class="npc-name-row">
							<span class="npc-mood"><MoodIcon mood={npc.mood} /></span>
							<span class="npc-name">{npc.name}</span>
						</div>
						{#if npc.introduced}
							<span class="npc-detail">{npc.occupation}</span>
						{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="empty">Nobody nearby.</p>
		{/if}
	</details>

	<details open>
		<summary>{$uiConfig.hints_label}</summary>
		{#if $nameHints.length > 0 || $languageHints.length > 0}
			<ul class="hint-list">
				{#each $nameHints as hint}
					<li class="hint-item name-hint">
						<span class="word">{hint.word}</span>
						<span class="pronunciation">[{hint.pronunciation}]</span>
						{#if hint.meaning}
							<span class="meaning">— {hint.meaning}</span>
						{/if}
					</li>
				{/each}
				{#each $languageHints as hint}
					<li class="hint-item">
						<span class="word">{hint.word}</span>
						<span class="pronunciation">[{hint.pronunciation}]</span>
						{#if hint.meaning}
							<span class="meaning">— {hint.meaning}</span>
						{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="empty">No words yet.</p>
		{/if}
	</details>
</aside>

<style>
	.sidebar {
		background: var(--color-panel-bg);
		border-left: 1px solid var(--color-border);
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		min-height: 0;
	}

	details {
		border-bottom: 1px solid var(--color-border);
	}

	summary {
		padding: 0.55rem 0.75rem;
		font-family: var(--font-display);
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.13em;
		color: var(--color-muted);
		cursor: pointer;
		user-select: none;
		list-style: none;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	summary::before {
		content: '▸ ';
		font-size: 0.55rem;
		opacity: 0.7;
	}

	details[open] summary::before {
		content: '▾ ';
	}

	.npc-list,
	.hint-list {
		list-style: none;
		margin: 0;
		padding: 0.25rem 0;
	}

	.npc-item {
		padding: 0.4rem 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.1rem;
		border-bottom: 1px solid var(--color-border);
	}

	.npc-item:last-child {
		border-bottom: none;
	}

	.npc-name-row {
		display: flex;
		align-items: baseline;
		gap: 0.35rem;
	}

	.npc-name {
		color: var(--color-accent);
		font-style: italic;
		font-size: 0.9rem;
	}

	.npc-detail {
		color: var(--color-muted);
		font-size: 0.75rem;
	}

	.npc-mood {
		font-size: 1rem;
		cursor: default;
		display: inline-flex;
		align-self: center;
		transform: translateY(-2px);
	}

	.hint-item {
		padding: 0.4rem 0.75rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		align-items: baseline;
		border-bottom: 1px solid var(--color-border);
		font-size: 0.8rem;
	}

	.hint-item:last-child {
		border-bottom: none;
	}

	.word {
		color: var(--color-accent);
		font-weight: 600;
		font-style: italic;
	}

	.pronunciation {
		color: var(--color-muted);
	}

	.meaning {
		color: var(--color-fg);
		font-size: 0.75rem;
	}

	.name-hint .word {
		font-style: normal;
	}

	.empty {
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.8rem;
		padding: 0.5rem 0.75rem;
		margin: 0;
	}
</style>
