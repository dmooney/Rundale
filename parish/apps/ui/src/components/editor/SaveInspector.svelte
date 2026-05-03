<script lang="ts">
	import { onMount } from 'svelte';
	import {
		editorListSaves,
		editorListBranches,
		editorReadSnapshot
	} from '$lib/editor-ipc';
	import type { SaveFileSummary, BranchSummary, SnapshotDetail } from '$lib/editor-types';

	let saves: SaveFileSummary[] = $state([]);
	let selectedSave: SaveFileSummary | null = $state(null);
	let branches: BranchSummary[] = $state([]);
	let selectedBranch: BranchSummary | null = $state(null);
	let snapshot: SnapshotDetail | null = $state(null);
	let loading = $state(false);
	let error = $state('');

	async function refreshSaves() {
		loading = true;
		error = '';
		try {
			saves = await editorListSaves();
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	async function selectSave(save: SaveFileSummary) {
		selectedSave = save;
		selectedBranch = null;
		snapshot = null;
		loading = true;
		error = '';
		try {
			branches = await editorListBranches(save.path);
		} catch (e) {
			error = String(e);
			branches = [];
		} finally {
			loading = false;
		}
	}

	async function selectBranch(branch: BranchSummary) {
		if (!selectedSave) return;
		selectedBranch = branch;
		snapshot = null;
		loading = true;
		error = '';
		try {
			snapshot = await editorReadSnapshot(selectedSave.path, branch.id);
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	function exportSnapshot() {
		if (!snapshot) return;
		const json = JSON.stringify(snapshot.world_state, null, 4);
		const blob = new Blob([json], { type: 'application/json' });
		const url = URL.createObjectURL(blob);
		const a = document.createElement('a');
		a.href = url;
		a.download = `snapshot-${snapshot.branch_id}-${snapshot.id}.json`;
		a.click();
		URL.revokeObjectURL(url);
	}

	onMount(() => {
		refreshSaves();
	});

	function formatSnapshotPreview(world_state: unknown): string {
		try {
			return JSON.stringify(world_state, null, 4);
		} catch {
			return String(world_state);
		}
	}
</script>

<div class="save-inspector">
	<div class="panel-header">
		<h3 class="panel-title">Save Inspector</h3>
		<button class="refresh-btn" onclick={refreshSaves} disabled={loading}>
			{loading ? '...' : 'Refresh'}
		</button>
	</div>

	<div class="inspector-grid">
		<!-- Saves column -->
		<div class="col saves-col">
			<h4 class="col-title">Save Files ({saves.length})</h4>
			<div class="col-scroll">
				{#each saves as save (save.path)}
					<button
						class="col-item"
						class:active={selectedSave?.path === save.path}
						onclick={() => selectSave(save)}
					>
						<span class="item-name">{save.filename}</span>
						<span class="item-meta">{save.file_size} &middot; {save.branch_count} branches</span>
					</button>
				{/each}
				{#if saves.length === 0 && !loading}
					<p class="empty-note">No save files found.</p>
				{/if}
			</div>
		</div>

		<!-- Branches column -->
		<div class="col branches-col">
			<h4 class="col-title">Branches ({branches.length})</h4>
			<div class="col-scroll">
				{#each branches as branch (branch.id)}
					<button
						class="col-item"
						class:active={selectedBranch?.id === branch.id}
						onclick={() => selectBranch(branch)}
					>
						<span class="item-name">{branch.name}</span>
						<span class="item-meta">
							{branch.snapshot_count} snapshots
							{#if branch.parent_branch_name}
								&middot; from {branch.parent_branch_name}
							{/if}
						</span>
					</button>
				{/each}
				{#if selectedSave && branches.length === 0 && !loading}
					<p class="empty-note">No branches.</p>
				{/if}
				{#if !selectedSave}
					<p class="empty-note">Select a save file.</p>
				{/if}
			</div>
		</div>

		<!-- Snapshot column -->
		<div class="col snapshot-col">
			<div class="snapshot-header">
				<h4 class="col-title">Latest Snapshot</h4>
				{#if snapshot}
					<button class="export-btn" onclick={exportSnapshot}>Export JSON</button>
				{/if}
			</div>
			<div class="col-scroll">
				{#if snapshot}
					<div class="snapshot-meta">
						<div><strong>Game time:</strong> {snapshot.game_time}</div>
						<div><strong>Real time:</strong> {snapshot.real_time}</div>
						<div><strong>Snapshot ID:</strong> {snapshot.id}</div>
					</div>
					<pre class="snapshot-body">{formatSnapshotPreview(snapshot.world_state)}</pre>
				{:else if selectedBranch && !loading}
					<p class="empty-note">No snapshots on this branch.</p>
				{:else if !selectedBranch}
					<p class="empty-note">Select a branch.</p>
				{/if}
			</div>
		</div>
	</div>

	{#if error}
		<div class="error">{error}</div>
	{/if}
</div>

<style>
	.save-inspector {
		height: 100%;
		display: flex;
		flex-direction: column;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0.5rem 1rem;
		border-bottom: 1px solid var(--color-border);
	}

	.panel-title {
		font-family: 'Cinzel', serif;
		font-size: 0.95rem;
		margin: 0;
		color: var(--color-accent);
	}

	.refresh-btn,
	.export-btn {
		padding: 0.25rem 0.6rem;
		border: 1px solid var(--color-accent);
		border-radius: 3px;
		background: none;
		color: var(--color-accent);
		font-size: 0.7rem;
		font-family: 'IM Fell English', serif;
		cursor: pointer;
	}
	.refresh-btn:hover,
	.export-btn:hover {
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
	}
	.refresh-btn:disabled {
		opacity: 0.5;
	}

	.inspector-grid {
		flex: 1;
		display: grid;
		grid-template-columns: 1fr 1fr 2fr;
		overflow: hidden;
	}

	.col {
		display: flex;
		flex-direction: column;
		border-right: 1px solid var(--color-border);
		overflow: hidden;
	}

	.col:last-child {
		border-right: none;
	}

	.col-title {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-muted);
		margin: 0;
		padding: 0.5rem 0.8rem;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-panel-bg);
	}

	.snapshot-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-panel-bg);
		padding-right: 0.6rem;
	}
	.snapshot-header .col-title {
		border-bottom: none;
	}

	.col-scroll {
		flex: 1;
		overflow-y: auto;
	}

	.col-item {
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
	.col-item:hover {
		background: var(--color-input-bg);
	}
	.col-item.active {
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

	.snapshot-meta {
		padding: 0.6rem 0.8rem;
		font-size: 0.75rem;
		border-bottom: 1px solid var(--color-border);
	}
	.snapshot-meta div {
		margin-bottom: 0.2rem;
	}

	.snapshot-body {
		margin: 0;
		padding: 0.6rem 0.8rem;
		font-family: monospace;
		font-size: 0.65rem;
		white-space: pre;
		overflow-x: auto;
		color: var(--color-fg);
	}

	.empty-note {
		padding: 0.6rem 0.8rem;
		color: var(--color-muted);
		font-size: 0.75rem;
		font-style: italic;
	}

	.error {
		padding: 0.4rem 1rem;
		color: #ff4444;
		font-size: 0.75rem;
		border-top: 1px solid var(--color-border);
	}
</style>
