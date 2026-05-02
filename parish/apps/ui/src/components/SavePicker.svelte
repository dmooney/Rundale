<script lang="ts">
	import { tick } from 'svelte';
	import { savePickerVisible, saveFiles, currentSaveState } from '../stores/save';
	import { discoverSaveFiles, loadBranch, saveGame, newSaveFile, newGame, createBranch, getSaveState, getWorldSnapshot, getMap, getNpcsHere } from '$lib/ipc';
	import { worldState, mapData, npcsHere } from '../stores/game';
	import type { SaveFileInfo, SaveBranchDisplay } from '$lib/types';
	import { layoutTree, NODE_W, NODE_H, GAP_Y } from '$lib/save-picker/dag';

	let loadingCount = 0;
	$: loading = loadingCount > 0;
	let forkingBranchId: number | null = null;
	let forkName = '';
	let forkError = '';
	let showLedgers = false;

	$: activeFile = files.find(f => f.filename === saveState?.filename) ?? files[0] ?? null;

	// ── Handlers ────────────────────────────────────────────────────

	async function refreshSaves() {
		loadingCount++;
		try {
			const allFiles = await discoverSaveFiles();
			saveFiles.set(allFiles);
			const state = await getSaveState();
			currentSaveState.set(state);
		} catch (e) {
			console.error('Failed to discover saves:', e);
		}
		loadingCount--;
	}

	async function refreshGameState() {
		try {
			const [ws, md, npcs] = await Promise.all([
				getWorldSnapshot(),
				getMap(),
				getNpcsHere()
			]);
			worldState.set(ws);
			mapData.set(md);
			npcsHere.set(npcs);
		} catch (e) {
			console.error('Failed to refresh game state:', e);
		}
	}

	async function handleLoadBranch(file: SaveFileInfo, branch: SaveBranchDisplay) {
		loadingCount++;
		try {
			await loadBranch(file.path, branch.id);
			await refreshGameState();
			savePickerVisible.set(false);
		} catch (e) {
			console.error('Load failed:', e);
		}
		loadingCount--;
	}

	async function handleForkLedger() {
		loadingCount++;
		try {
			await newSaveFile();
			await refreshGameState();
			showLedgers = false;
			savePickerVisible.set(false);
		} catch (e) {
			console.error('Fork ledger failed:', e);
		}
		loadingCount--;
	}

	async function handleNewGame() {
		loadingCount++;
		try {
			await newGame();
			await refreshGameState();
			showLedgers = false;
			savePickerVisible.set(false);
		} catch (e) {
			console.error('New game failed:', e);
		}
		loadingCount--;
	}

	async function handleSwitchLedger(file: SaveFileInfo) {
		const branch = file.branches[0];
		if (!branch) return;
		loadingCount++;
		try {
			await loadBranch(file.path, branch.id);
			await refreshGameState();
			showLedgers = false;
			await refreshSaves();
		} catch (e) {
			console.error('Switch ledger failed:', e);
		}
		loadingCount--;
	}

	async function handleFork(parentBranch: SaveBranchDisplay) {
		const name = forkName.trim();
		if (!name) return;
		loadingCount++;
		try {
			await createBranch(name, parentBranch.id);
			forkingBranchId = null;
			forkName = '';
			// Save scroll position before refresh re-renders the tree
			const body = document.querySelector('.modal-body');
			const scrollTop = body?.scrollTop ?? 0;
			const scrollLeft = body?.scrollLeft ?? 0;
			await refreshSaves();
			// Restore scroll position after re-render
			requestAnimationFrame(() => {
				if (body) {
					body.scrollTop = scrollTop;
					body.scrollLeft = scrollLeft;
				}
			});
		} catch (e: any) {
			console.error('Branch creation failed:', e);
			forkError = String(e).substring(0, 60);
		}
		loadingCount--;
	}

	/** Generate a default branch name based on the parent branch's state. */
	function generateBranchName(parent: SaveBranchDisplay, branches: SaveBranchDisplay[]): string {
		const existing = new Set(branches.map(b => b.name));
		// Try location-based name first
		if (parent.latest_location) {
			const locSlug = parent.latest_location.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
			if (!existing.has(locSlug)) return locSlug;
			for (let i = 2; i < 100; i++) {
				const name = `${locSlug}-${i}`;
				if (!existing.has(name)) return name;
			}
		}
		// Fallback: numbered
		for (let i = 1; i < 100; i++) {
			const name = `branch-${i}`;
			if (!existing.has(name)) return name;
		}
		return `branch-${Date.now()}`;
	}

	function startFork(branchId: number) {
		if (!activeFile) return;
		const parent = activeFile.branches.find(b => b.id === branchId);
		if (!parent) return;
		forkingBranchId = branchId;
		forkName = generateBranchName(parent, activeFile.branches);
		forkError = '';
	}

	function autofocus(node: HTMLInputElement) {
		node.focus();
		node.select();
		// Scroll the phantom node into view with extra room for scrollbar
		requestAnimationFrame(() => {
			const dagNode = node.closest('.dag-node') as HTMLElement | null;
			const body = document.querySelector('.modal-body');
			if (dagNode && body) {
				const nodeRect = dagNode.getBoundingClientRect();
				const bodyRect = body.getBoundingClientRect();
				const scrollPad = 30;
				// Scroll up if node is above visible area
				if (nodeRect.top < bodyRect.top + scrollPad) {
					body.scrollTop -= (bodyRect.top + scrollPad - nodeRect.top);
				}
				// Scroll down if node is below visible area
				if (nodeRect.bottom > bodyRect.bottom - scrollPad) {
					body.scrollTop += (nodeRect.bottom - bodyRect.bottom + scrollPad);
				}
				// Scroll right if node is past visible area
				if (nodeRect.right > bodyRect.right - scrollPad) {
					body.scrollLeft += (nodeRect.right - bodyRect.right + scrollPad);
				}
			}
		});
	}

	function cancelFork() {
		forkingBranchId = null;
		forkName = '';
		forkError = '';
	}

	function close() {
		savePickerVisible.set(false);
		forkingBranchId = null;
		forkName = '';
		forkError = '';
		showLedgers = false;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			if (forkingBranchId !== null) {
				cancelFork();
			} else if (showLedgers) {
				showLedgers = false;
			} else {
				close();
			}
		}
	}

	async function scrollToCurrentNode() {
		await tick();
		const current = document.querySelector('.dag-current');
		if (current) {
			current.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });
		}
	}

	let prevVisible = false;
	$: {
		const visible = $savePickerVisible;
		if (visible && !prevVisible) {
			refreshSaves().then(scrollToCurrentNode);
		}
		prevVisible = visible;
	}

	$: files = $saveFiles;
	$: saveState = $currentSaveState;

	// Phantom branch ID used to identify the new-branch node in the layout
	const PHANTOM_ID = -999;

	$: layoutBranches = (() => {
		if (!activeFile) return [];
		const branches = [...activeFile.branches];
		if (forkingBranchId !== null) {
			const parent = branches.find(b => b.id === forkingBranchId);
			if (parent) {
				branches.push({
					name: forkName || 'new-branch',
					id: PHANTOM_ID,
					parent_name: parent.name,
					snapshot_count: 0,
					latest_location: parent.latest_location,
					latest_game_date: parent.latest_game_date,
					snapshots: [],
				});
			}
		}
		return branches;
	})();
	$: layout = layoutBranches.length > 0 ? layoutTree(layoutBranches) : null;
</script>

<svelte:window on:keydown={handleKeydown} />

{#if $savePickerVisible}
	<div class="overlay" role="dialog" aria-modal="true" aria-label="The Parish Ledger">
		<div class="modal">
			<div class="modal-header">
				<span class="modal-title">
					{#if showLedgers}
						Ledgers
					{:else}
						The Parish Ledger
					{/if}
				</span>
			</div>

			<div class="modal-body">
				{#if loading && files.length === 0}
					<div class="loading-msg">Scanning save files...</div>
				{/if}

				{#if showLedgers}
					{#each files as file, fileIdx}
						{@const isActive = file.filename === saveState?.filename}
						<div class="ledger-row" class:ledger-active={isActive}>
							<span class="file-number">{fileIdx + 1}.</span>
							<span class="file-name">{file.filename}</span>
							<span class="ledger-meta">
								{file.file_size}
								{#if file.branches[0]?.latest_location}
									— {file.branches[0].latest_location}
								{/if}
							</span>
							{#if isActive}
								<span class="ledger-current">You are here</span>
							{:else if file.locked}
								<span class="ledger-locked">In Use</span>
							{:else}
								<button class="action-btn" on:click={() => handleSwitchLedger(file)} disabled={loading}>Open</button>
							{/if}
						</div>
					{/each}

					<div class="ledger-row new-ledger" on:click={() => { if (!loading) handleForkLedger(); }} role="button" tabindex="0" aria-disabled={loading} on:keydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (!loading && !e.repeat) handleForkLedger(); } }}>
						<span class="file-number">+</span>
						<span class="file-name">Fork New Ledger</span>
					</div>

					<div class="ledger-row new-ledger" on:click={() => { if (!loading) handleNewGame(); }} role="button" tabindex="0" aria-disabled={loading} on:keydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); if (!loading && !e.repeat) handleNewGame(); } }}>
						<span class="file-number">+</span>
						<span class="file-name">New Game</span>
					</div>
				{:else if layout && activeFile}
					<!-- Inverted DAG tree -->
					<div class="dag-scroll">
						<div class="dag-container" style="width: {layout.width}px; height: {layout.height}px;">
							<!-- Connection lines -->
							<svg class="dag-edges" width={layout.width} height={layout.height}>
								{#each layout.edges as edge}
									<path
										d="M {edge.x1} {edge.y1} C {edge.x1} {edge.y1 - GAP_Y * 0.5}, {edge.x2} {edge.y2 + GAP_Y * 0.5}, {edge.x2} {edge.y2}"
										fill="none"
										stroke="var(--color-border)"
										stroke-width="1.5"
									/>
								{/each}
							</svg>

							<!-- Node boxes -->
							{#each layout.nodes as node (node.branch.id)}
								{#if node.branch.id === PHANTOM_ID}
									<!-- Phantom node: editable new branch -->
									{@const parent = activeFile.branches.find(b => b.id === forkingBranchId)}
									<div
										class="dag-node dag-phantom"
										style="left: {node.x}px; top: {node.y}px; width: {NODE_W}px; height: {NODE_H}px;"
									>
										<div class="phantom-body">
											<input
												class="phantom-name-input"
												type="text"
												bind:value={forkName}
												use:autofocus
												on:keydown|stopPropagation={(e) => { if (e.key === 'Enter' && parent) { e.preventDefault(); handleFork(parent); } if (e.key === 'Escape') cancelFork(); }}
												on:input={() => { forkError = ''; }}
											/>
											{#if forkError}
												<span class="fork-error">{forkError}</span>
											{:else}
												<span class="node-location">{node.branch.latest_location ?? 'New'}</span>
											{/if}
											<div class="phantom-actions">
												<button class="phantom-btn" on:click|stopPropagation={() => { if (parent) handleFork(parent); }} disabled={loading || !forkName.trim()}>Create</button>
												<button class="phantom-btn" on:click|stopPropagation={cancelFork}>Cancel</button>
											</div>
										</div>
									</div>
								{:else}
									{@const isCurrent = node.branch.name === saveState?.branch_name}
									<div
										class="dag-node"
										class:dag-current={isCurrent}
										style="left: {node.x}px; top: {node.y}px; width: {NODE_W}px; height: {NODE_H}px;"
									>
										<button
											class="node-body"
											disabled={loading}
											on:click={() => handleLoadBranch(activeFile, node.branch)}
										>
											<span class="node-name">{node.branch.name}</span>
											<span class="node-location">{node.branch.latest_location ?? 'New'}</span>
											<span class="node-date">{node.branch.latest_game_date ?? ''}</span>
										</button>
										{#if isCurrent}
											<span class="node-current-badge">You are here</span>
										{/if}
										<button
											class="node-branch-btn"
											disabled={loading}
											on:click|stopPropagation={() => startFork(node.branch.id)}
										>Branch From Here</button>
									</div>
								{/if}
							{/each}
						</div>
					</div>
				{:else}
					<div class="loading-msg">No save file found.</div>
				{/if}
			</div>

			<div class="modal-footer">
				{#if showLedgers}
					<button class="footer-btn" on:click={() => { showLedgers = false; }}>
						← Back
					</button>
				{:else}
					<button class="footer-btn" on:click={() => { showLedgers = true; }}>
						Ledgers
					</button>
				{/if}
				<span class="footer-spacer"></span>
				<button class="footer-btn" on:click={close}>Close</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 1000;
	}

	.modal {
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		max-width: 85vw;
		width: 90%;
		height: 67vh;
		display: flex;
		flex-direction: column;
		border-radius: 2px;
	}

	.modal-header {
		padding: 0.6rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.modal-title {
		font-size: 0.75rem;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--color-accent);
	}

	.modal-body {
		flex: 1;
		overflow: auto;
		padding: 0.75rem;
		min-height: 0;
		scrollbar-width: thin;
		scrollbar-color: var(--color-border) transparent;
	}
	.modal-body::-webkit-scrollbar {
		width: 6px;
		height: 6px;
	}
	.modal-body::-webkit-scrollbar-thumb {
		background: var(--color-border);
		border-radius: 3px;
	}
	.modal-body::-webkit-scrollbar-track {
		background: transparent;
	}
	.modal-body::-webkit-scrollbar-corner {
		background: transparent;
	}

	.modal-footer {
		padding: 0.4rem 0.75rem;
		border-top: 1px solid var(--color-border);
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.footer-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.65rem;
		padding: 0.15rem 0.5rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.footer-spacer {
		flex: 1;
	}
	.footer-btn:hover,
	.footer-btn:focus-visible {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	/* ── DAG tree ────────────────────────────────────────────────── */

	.dag-scroll {
		padding: 1rem;
	}

	.dag-container {
		position: relative;
		margin: auto auto 0 auto;
	}

	.dag-edges {
		position: absolute;
		top: 0;
		left: 0;
		pointer-events: none;
	}

	.dag-node {
		position: absolute;
		border: 1px solid var(--color-border);
		background: var(--color-panel-bg);
		box-sizing: border-box;
		padding-top: 0;
	}
	.dag-node::before {
		content: '';
		position: absolute;
		top: -24px;
		left: 0;
		right: 0;
		height: 24px;
	}
	.dag-node:hover {
		border-color: var(--color-accent);
	}
	.dag-node.dag-current {
		border-color: var(--color-accent);
		border-width: 2px;
	}

	.node-body {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.15rem;
		padding: 0.3rem 0.5rem;
		width: 100%;
		height: 100%;
		background: none;
		border: none;
		color: var(--color-fg);
		cursor: pointer;
		text-align: center;
		box-sizing: border-box;
	}
	.node-body:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.node-branch-btn {
		display: none;
		position: absolute;
		bottom: 100%;
		left: 50%;
		transform: translateX(-50%);
		background: var(--color-panel-bg);
		backdrop-filter: blur(4px);
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.6rem;
		padding: 0.15rem 0.4rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		white-space: nowrap;
		margin-bottom: 4px;
		z-index: 5;
	}
	.dag-node:hover .node-branch-btn,
	.dag-node:focus-within .node-branch-btn {
		display: block;
	}
	.node-branch-btn:hover,
	.node-branch-btn:focus-visible {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}
	.node-branch-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.node-name {
		font-size: 0.75rem;
		font-weight: bold;
		color: var(--color-accent);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}

	.node-location {
		font-size: 0.6rem;
		color: var(--color-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}

	.node-date {
		font-size: 0.55rem;
		color: var(--color-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}

	.node-current-badge {
		position: absolute;
		bottom: -0.5rem;
		right: 0.3rem;
		font-size: 0.65rem;
		color: var(--color-accent);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		font-weight: bold;
		background: var(--color-panel-bg);
		padding: 0 0.25rem;
	}

	.dag-phantom {
		border-style: dashed;
		border-color: var(--color-accent);
	}

	.phantom-body {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.15rem;
		padding: 0.25rem 0.4rem;
		width: 100%;
		height: 100%;
		box-sizing: border-box;
	}

	.phantom-name-input {
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-accent);
		font-size: 0.7rem;
		font-weight: bold;
		padding: 0.1rem 0.3rem;
		text-align: center;
		width: 90%;
	}
	.phantom-name-input:focus {
		border-color: var(--color-accent);
		outline: none;
	}

	.fork-error {
		font-size: 0.55rem;
		color: #c44;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 100%;
	}

	.phantom-actions {
		display: flex;
		gap: 0.25rem;
	}

	.phantom-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.5rem;
		padding: 0.1rem 0.3rem;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}
	.phantom-btn:hover:not(:disabled),
	.phantom-btn:focus-visible:not(:disabled) {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}
	.phantom-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}

	/* ── Ledger view ─────────────────────────────────────────────── */

	.ledger-row {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		padding: 0.35rem 0.5rem;
		font-size: 0.8rem;
		border-bottom: 1px solid var(--color-border);
	}
	.ledger-row:last-child {
		border-bottom: none;
	}
	.ledger-row:hover {
		background: var(--color-input-bg);
	}
	.ledger-row.ledger-active {
		background: var(--color-input-bg);
	}

	.file-number {
		color: var(--color-muted);
		font-size: 0.8rem;
		flex-shrink: 0;
	}

	.file-name {
		color: var(--color-accent);
		font-size: 0.85rem;
		flex-shrink: 0;
	}

	.ledger-meta {
		color: var(--color-muted);
		font-size: 0.75rem;
		flex: 1;
	}

	.ledger-current {
		font-size: 0.6rem;
		color: var(--color-muted);
		font-style: italic;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.ledger-locked {
		font-size: 0.6rem;
		color: var(--color-muted);
		font-style: italic;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		opacity: 0.6;
	}

	.new-ledger {
		border-bottom: none;
		cursor: pointer;
	}
	.new-ledger:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: -2px;
	}
	.new-ledger[aria-disabled='true'] {
		opacity: 0.5;
		cursor: default;
	}

	.loading-msg {
		color: var(--color-muted);
		font-size: 0.8rem;
		font-style: italic;
		padding: 1rem 0;
		text-align: center;
	}

	.action-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		font-size: 0.6rem;
		padding: 0.15rem 0.4rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.action-btn:hover:not(:disabled),
	.action-btn:focus-visible:not(:disabled) {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}
	.action-btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
