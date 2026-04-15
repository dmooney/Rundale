<script lang="ts">
	import { languageHints, nameHints, uiConfig } from '../stores/game';

	let { onclose }: { onclose?: () => void } = $props();
</script>

{#if onclose}
	<div class="focail-panel">
		<div class="panel-header">
			<span class="panel-title"><span class="panel-title-word">Focail Gaeilge</span> <span class="panel-title-label">(Irish Words)</span></span>
			<button type="button" class="close-btn" aria-label="Close Irish words panel" title="Close" onclick={onclose}><span aria-hidden="true">&times;</span></button>
		</div>
		<div class="panel-content">
			{#if $nameHints.length > 0 || $languageHints.length > 0}
				<ul class="hint-list">
					{#each $nameHints as hint}
						<li class="hint-item name-hint hint-name">
							<span class="word">{hint.word}</span>
							<span class="pronunciation">[{hint.pronunciation}]</span>
							{#if hint.meaning}
								<span class="meaning">— {hint.meaning}</span>
							{/if}
						</li>
					{/each}
					{#each $languageHints as hint}
						<li class="hint-item hint-irish">
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
		</div>
	</div>
{:else}
	<aside class="sidebar" data-testid="sidebar">
		<details open>
			<summary>{$uiConfig.hints_label}</summary>
			{#if $nameHints.length > 0 || $languageHints.length > 0}
				<ul class="hint-list">
					{#each $nameHints as hint}
						<li class="hint-item name-hint hint-name">
							<span class="word">{hint.word}</span>
							<span class="pronunciation">[{hint.pronunciation}]</span>
							{#if hint.meaning}
								<span class="meaning">— {hint.meaning}</span>
							{/if}
						</li>
					{/each}
					{#each $languageHints as hint}
						<li class="hint-item hint-irish">
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
{/if}

<style>
	.focail-panel {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--color-panel-bg);
	}

	.panel-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.panel-title {
		flex: 1;
		display: flex;
		align-items: baseline;
		gap: 0.4em;
	}

	.panel-title-word {
		color: var(--color-accent);
		font-weight: 600;
		font-style: italic;
		font-size: 0.85rem;
	}

	.panel-title-label {
		font-family: var(--font-display);
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.13em;
		color: var(--color-muted);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-muted);
		font-size: 1.4rem;
		cursor: pointer;
		padding: 0 4px;
		line-height: 1;
	}

	.close-btn:hover,
	.close-btn:focus-visible {
		color: var(--color-fg);
	}

	.close-btn:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 2px;
		border-radius: 2px;
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
	}

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
		font-weight: 600;
		font-style: italic;
	}

	.hint-irish .word {
		color: var(--color-irish);
	}

	.hint-name .word {
		color: var(--color-name);
		font-style: normal;
	}

	.pronunciation {
		color: var(--color-muted);
	}

	.meaning {
		color: var(--color-fg);
		font-size: 0.75rem;
	}

	.empty {
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.8rem;
		padding: 0.5rem 0.75rem;
		margin: 0;
	}
</style>
