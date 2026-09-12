<script lang="ts">
	import type { DebugSnapshot } from '$lib/types';

	let { snap }: { snap: DebugSnapshot } = $props();
</script>

<div class="section">
	<h4>Gossip Network ({snap.gossip.item_count})</h4>
	{#if snap.gossip.items.length === 0}
		<div class="field muted">(no gossip)</div>
	{:else}
		{#each snap.gossip.items as item (item.id)}
			<div class="gossip-item">
				<div class="field">
					<span class="muted">#{item.id}</span>
					{#if item.distortion_level > 0}
						<span class="accent">[distortion {item.distortion_level}]</span>
					{/if}
					{item.content}
				</div>
				<div class="field muted indent">source: {item.source_name} | known by {item.known_by.length}: {item.known_by.join(', ')}</div>
				<div class="field muted indent">at {item.timestamp}</div>
			</div>
		{/each}
	{/if}
</div>

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.accent { color: var(--color-accent); }
	.muted { color: var(--color-muted); }
	.indent { padding-left: 1rem; }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }

	.gossip-item {
		margin-bottom: 0.3rem;
		padding-bottom: 0.3rem;
		border-bottom: 1px dashed var(--color-border);
	}
</style>
