<script lang="ts">
	import { npcsHere, irishHints } from '../stores/game';
</script>

<aside class="sidebar">
	<details open>
		<summary>NPCs Here</summary>
		{#if $npcsHere.length > 0}
			<ul class="npc-list">
				{#each $npcsHere as npc}
					<li class="npc-item">
						<span class="npc-name">{npc.name}</span>
						{#if npc.introduced}
							<span class="npc-detail">{npc.occupation}</span>
							<span class="npc-mood">{npc.mood}</span>
						{/if}
					</li>
				{/each}
			</ul>
		{:else}
			<p class="empty">Nobody nearby.</p>
		{/if}
	</details>

	<details open>
		<summary>Focail (Irish Words)</summary>
		{#if $irishHints.length > 0}
			<ul class="hint-list">
				{#each $irishHints as hint}
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
			<p class="empty">No Irish words yet.</p>
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
		padding: 0.5rem 0.75rem;
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-muted);
		cursor: pointer;
		user-select: none;
		list-style: none;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	summary::before {
		content: '▶ ';
		font-size: 0.6rem;
	}

	details[open] summary::before {
		content: '▼ ';
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

	.npc-name {
		color: var(--color-accent);
		font-weight: 600;
		font-size: 0.85rem;
	}

	.npc-detail,
	.npc-mood {
		color: var(--color-muted);
		font-size: 0.75rem;
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

	.empty {
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.8rem;
		padding: 0.5rem 0.75rem;
		margin: 0;
	}
</style>
