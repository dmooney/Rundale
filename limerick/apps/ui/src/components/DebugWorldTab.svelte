<script lang="ts">
	import type { DebugSnapshot } from '$lib/types';

	let { snap }: { snap: DebugSnapshot } = $props();
</script>

<div class="section">
	<h4>Locations ({snap.world.visited_count}/{snap.world.location_count} visited)</h4>
	{#each snap.world.locations as loc (loc.id)}
		<div class="loc-row" class:player-here={loc.id === snap.world.player_location_id}>
			<div class="field">
				{#if loc.id === snap.world.player_location_id}<strong>>>> </strong>{/if}
				{loc.name}
				<span class="muted">({loc.indoor ? 'indoor' : 'outdoor'}/{loc.public ? 'pub' : 'prv'}, {loc.connection_count} exits{#if !loc.visited}, unvisited{/if})</span>
			</div>
			{#if loc.npcs_here.length > 0}
				<div class="field muted indent">NPCs: {loc.npcs_here.join(', ')}</div>
			{/if}
			{#if loc.edges.length > 0}
				{#each loc.edges as edge, ei (ei)}
					<div class="field muted indent">\u2192 {edge.target_name} ({edge.walking_minutes}m walk) \u2014 {edge.path_description}</div>
				{/each}
			{/if}
		</div>
	{/each}
</div>
{#if snap.world.edge_traversals.length > 0}
	<div class="section">
		<h4>Worn Paths (top edges)</h4>
		{#each snap.world.edge_traversals.slice(0, 20) as edge, ti (ti)}
			<div class="field">{edge.from_name} \u2194 {edge.to_name} <span class="muted">\u00d7{edge.count}</span></div>
		{/each}
	</div>
{/if}
<div class="section">
	<h4>Text Log (tail {snap.world.text_log_tail.length}/{snap.world.text_log_len})</h4>
	{#if snap.world.text_log_tail.length === 0}
		<div class="field muted">(empty)</div>
	{:else}
		{#each snap.world.text_log_tail as line, li (li)}
			<div class="field muted">{line}</div>
		{/each}
	{/if}
</div>

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.muted { color: var(--color-muted); }
	.indent { padding-left: 1rem; }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }

	.player-here {
		background: color-mix(in srgb, var(--color-accent) 8%, transparent);
	}

	.loc-row {
		padding: 0.2rem 0;
		border-bottom: 1px solid var(--color-border);
	}
</style>
