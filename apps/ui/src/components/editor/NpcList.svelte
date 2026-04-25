<script lang="ts">
	import { editorNpcs, editorSelectedNpcId, editorLocations } from '../../stores/editor';

	let search = '';

	$: npcs = $editorNpcs;
	$: locations = $editorLocations;
	$: filtered = search
		? npcs.filter(
				(n) =>
					n.name.toLowerCase().includes(search.toLowerCase()) ||
					n.occupation.toLowerCase().includes(search.toLowerCase())
			)
		: npcs;

	function locationName(id: number): string {
		return locations.find((l) => l.id === id)?.name ?? `#${id}`;
	}
</script>

<div class="npc-list">
	<div class="list-header">
		<h3 class="list-title">NPCs ({npcs.length})</h3>
		<input
			class="search-input"
			type="text"
			placeholder="Search..."
			aria-label="Search NPCs"
			bind:value={search}
		/>
	</div>
	<div class="list-scroll">
		{#each filtered as npc (npc.id)}
			<button
				class="list-item"
				class:active={$editorSelectedNpcId === npc.id}
				on:click={() => editorSelectedNpcId.set(npc.id)}
			>
				<span class="item-name">{npc.name}</span>
				<span class="item-meta">{npc.occupation} &middot; {locationName(npc.home)}</span>
			</button>
		{/each}
	</div>
</div>

<style>
	.npc-list {
		width: 260px;
		min-width: 200px;
		border-right: 1px solid var(--color-border);
		display: flex;
		flex-direction: column;
		background: var(--color-panel-bg);
	}

	.list-header {
		padding: 0.5rem 0.6rem;
		border-bottom: 1px solid var(--color-border);
	}

	.list-title {
		font-size: 0.75rem;
		color: var(--color-muted);
		margin: 0 0 0.3rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.search-input {
		width: 100%;
		padding: 0.25rem 0.4rem;
		border: 1px solid var(--color-border);
		border-radius: 3px;
		background: var(--color-input-bg);
		color: var(--color-fg);
		font-size: 0.75rem;
		font-family: 'IM Fell English', serif;
		box-sizing: border-box;
	}

	.list-scroll {
		flex: 1;
		overflow-y: auto;
	}

	.list-item {
		display: flex;
		flex-direction: column;
		gap: 0.05rem;
		width: 100%;
		padding: 0.4rem 0.6rem;
		border: none;
		border-bottom: 1px solid var(--color-border);
		background: none;
		cursor: pointer;
		text-align: left;
		font-family: 'IM Fell English', serif;
		color: var(--color-fg);
	}
	.list-item:hover {
		background: var(--color-input-bg);
	}
	.list-item.active {
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
		border-left: 2px solid var(--color-accent);
	}

	.item-name {
		font-size: 0.8rem;
		font-weight: 600;
	}

	.item-meta {
		font-size: 0.65rem;
		color: var(--color-muted);
	}
</style>
