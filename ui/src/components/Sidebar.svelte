<script lang="ts">
	import { languageHints, nameHints, uiConfig } from '../stores/game';
</script>

<aside class="sidebar" data-testid="sidebar">
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

	.hint-list {
		list-style: none;
		margin: 0;
		padding: 0.25rem 0;
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
