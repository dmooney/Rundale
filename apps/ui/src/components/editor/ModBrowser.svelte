<script lang="ts">
	import { editorMods, editorSnapshot, editorTab, editorValidation } from '../../stores/editor';
	import { editorOpenMod } from '$lib/editor-ipc';
	import type { ModSummary } from '$lib/editor-types';

	let loading = false;
	let error = '';

	async function openMod(mod_summary: ModSummary) {
		loading = true;
		error = '';
		try {
			const snapshot = await editorOpenMod(mod_summary.path);
			editorSnapshot.set(snapshot);
			editorValidation.set(snapshot.validation);
			editorTab.set('npcs');
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	$: mods = $editorMods;
</script>

<div class="mod-browser">
	<h2 class="section-title">Select a Mod</h2>

	{#if mods.length === 0}
		<p class="empty">No mods found in the <code>mods/</code> directory.</p>
	{:else}
		<div class="mod-list">
			{#each mods as mod_item}
				<button
					class="mod-card"
					on:click={() => openMod(mod_item)}
					disabled={loading}
				>
					<span class="mod-card-name">{mod_item.title ?? mod_item.name}</span>
					<span class="mod-card-version">v{mod_item.version}</span>
					<span class="mod-card-desc">{mod_item.description}</span>
					<span class="mod-card-id">{mod_item.id}</span>
				</button>
			{/each}
		</div>
	{/if}

	{#if error}
		<p class="error">{error}</p>
	{/if}
</div>

<style>
	.mod-browser {
		padding: 2rem;
		max-width: 600px;
		margin: 0 auto;
	}

	.section-title {
		font-family: 'Cinzel', serif;
		font-size: 1rem;
		color: var(--color-accent);
		margin: 0 0 1rem;
	}

	.empty {
		color: var(--color-muted);
		font-size: 0.85rem;
	}

	.mod-list {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.mod-card {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		padding: 0.75rem 1rem;
		border: 1px solid var(--color-border);
		border-radius: 4px;
		background: var(--color-panel-bg);
		cursor: pointer;
		text-align: left;
		font-family: 'IM Fell English', serif;
		color: var(--color-fg);
	}
	.mod-card:hover {
		border-color: var(--color-accent);
	}
	.mod-card:disabled {
		opacity: 0.5;
		cursor: wait;
	}

	.mod-card-name {
		font-family: 'Cinzel', serif;
		font-size: 0.95rem;
		color: var(--color-accent);
	}

	.mod-card-version {
		font-size: 0.7rem;
		color: var(--color-muted);
	}

	.mod-card-desc {
		font-size: 0.8rem;
	}

	.mod-card-id {
		font-size: 0.65rem;
		color: var(--color-muted);
	}

	.error {
		color: #ff4444;
		font-size: 0.8rem;
		margin-top: 1rem;
	}
</style>
