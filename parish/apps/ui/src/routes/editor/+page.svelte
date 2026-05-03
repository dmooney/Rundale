<script lang="ts">
	import { onMount } from 'svelte';
	import { editorListMods } from '$lib/editor-ipc';
	import {
		editorMods,
		editorSnapshot,
		editorTab,
		editorDirty,
		editorValidation,
		editorIssueCount,
		editorSelectedNpcId,
		editorSelectedLocationId
	} from '../../stores/editor';
	import type { EditorTab } from '$lib/editor-types';

	import ModBrowser from '../../components/editor/ModBrowser.svelte';
	import NpcList from '../../components/editor/NpcList.svelte';
	import NpcDetail from '../../components/editor/NpcDetail.svelte';
	import LocationList from '../../components/editor/LocationList.svelte';
	import LocationDetail from '../../components/editor/LocationDetail.svelte';
	import ValidatorPanel from '../../components/editor/ValidatorPanel.svelte';
	import SaveInspector from '../../components/editor/SaveInspector.svelte';

	const tabs: { id: EditorTab; label: string }[] = [
		{ id: 'mods', label: 'Mods' },
		{ id: 'npcs', label: 'NPCs' },
		{ id: 'locations', label: 'Locations' },
		{ id: 'validator', label: 'Validator' },
		{ id: 'saves', label: 'Saves' }
	];

	function selectTab(id: EditorTab) {
		editorTab.set(id);
		editorSelectedNpcId.set(null);
		editorSelectedLocationId.set(null);
	}

	onMount(async () => {
		try {
			const mods = await editorListMods();
			editorMods.set(mods);
		} catch (e) {
			console.warn('Failed to list mods:', e);
		}
	});

	const snap = $derived($editorSnapshot);
	const tab = $derived($editorTab);
	const issueCount = $derived($editorIssueCount);
</script>

<div class="editor-page">
	<div class="editor-header">
		<a href="/" class="back-link">&larr; Game</a>
		<h1 class="editor-title">Parish Designer</h1>
		{#if snap}
			<span class="mod-name">{snap.manifest.name} v{snap.manifest.version}</span>
			{#if $editorDirty}
				<span class="dirty-dot" title="Unsaved changes">&bull;</span>
			{/if}
		{/if}
	</div>

	<div class="tab-bar">
		{#each tabs as t}
			{#if snap || t.id === 'mods' || t.id === 'saves'}
				<button
					class="tab-btn"
					class:active={tab === t.id}
					onclick={() => selectTab(t.id)}
				>
					{t.label}
					{#if t.id === 'validator' && issueCount > 0}
						<span class="badge">{issueCount}</span>
					{/if}
				</button>
			{/if}
		{/each}
	</div>

	<div class="tab-content">
		{#if tab === 'mods'}
			<ModBrowser />
		{:else if tab === 'saves'}
			<SaveInspector />
		{:else if !snap}
			<ModBrowser />
		{:else if tab === 'npcs'}
			<div class="split-pane">
				<NpcList />
				<NpcDetail />
			</div>
		{:else if tab === 'locations'}
			<div class="split-pane">
				<LocationList />
				<LocationDetail />
			</div>
		{:else if tab === 'validator'}
			<ValidatorPanel />
		{/if}
	</div>
</div>

<style>
	.editor-page {
		display: flex;
		flex-direction: column;
		height: 100vh;
		background: var(--color-bg);
		color: var(--color-fg);
		font-family: 'IM Fell English', serif;
	}

	.editor-header {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 0.5rem 1rem;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-panel-bg);
	}

	.back-link {
		color: var(--color-muted);
		text-decoration: none;
		font-size: 0.85rem;
	}
	.back-link:hover {
		color: var(--color-accent);
	}

	.editor-title {
		font-family: 'Cinzel', serif;
		font-size: 1.1rem;
		margin: 0;
		color: var(--color-accent);
	}

	.mod-name {
		font-size: 0.8rem;
		color: var(--color-muted);
	}

	.dirty-dot {
		color: var(--color-accent);
		font-size: 1.5rem;
		line-height: 1;
	}

	.tab-bar {
		display: flex;
		gap: 0;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-panel-bg);
		padding: 0 1rem;
	}

	.tab-btn {
		padding: 0.4rem 0.8rem;
		border: none;
		border-bottom: 2px solid transparent;
		background: none;
		cursor: pointer;
		font-size: 0.8rem;
		color: var(--color-muted);
		font-family: 'IM Fell English', serif;
	}
	.tab-btn:hover {
		color: var(--color-fg);
	}
	.tab-btn.active {
		color: var(--color-accent);
		border-bottom-color: var(--color-accent);
	}

	.badge {
		font-size: 0.6rem;
		padding: 0.05rem 0.3rem;
		border-radius: 8px;
		background: color-mix(in srgb, #ff4444 20%, transparent);
		color: #ff4444;
		font-weight: 700;
		margin-left: 0.3rem;
	}

	.tab-content {
		flex: 1;
		overflow: hidden;
	}

	.split-pane {
		display: flex;
		height: 100%;
	}
</style>
