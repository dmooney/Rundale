<script lang="ts">
	import type { NpcInfo } from '$lib/types';

	let { npcs, selectedIndex, onSelect, onHighlight }: {
		npcs: NpcInfo[];
		selectedIndex: number;
		onSelect: (name: string) => void;
		onHighlight: (index: number) => void;
	} = $props();
</script>

<ul id="mention-listbox" class="mention-dropdown" role="listbox" aria-label="Mention NPC">
	{#each npcs as npc, i}
		<li
			id="mention-option-{i}"
			role="option"
			aria-selected={i === selectedIndex}
			class="mention-item"
			class:selected={i === selectedIndex}
			onmousedown={(e) => { e.preventDefault(); onSelect(npc.name); }}
			onmouseenter={() => onHighlight(i)}
		>
			<span class="mention-name">{npc.name}</span>
			{#if npc.introduced}
				<span class="mention-detail">{npc.occupation}</span>
			{/if}
		</li>
	{/each}
</ul>

<style>
	.mention-dropdown {
		position: absolute;
		bottom: 100%;
		left: 0.75rem;
		right: 0.75rem;
		margin: 0;
		padding: 0.25rem 0;
		list-style: none;
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		box-shadow: 0 -2px 8px rgba(0, 0, 0, 0.3);
		max-height: 12rem;
		overflow-y: auto;
		z-index: 10;
	}

	.mention-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0.75rem;
		cursor: pointer;
		color: var(--color-fg);
		font-size: 0.9rem;
	}

	.mention-item.selected {
		background: var(--color-accent);
		color: var(--color-bg);
	}

	.mention-name {
		font-weight: 600;
	}

	.mention-detail {
		font-size: 0.8rem;
		opacity: 0.7;
	}

	.mention-item.selected .mention-detail {
		opacity: 0.85;
	}
</style>
