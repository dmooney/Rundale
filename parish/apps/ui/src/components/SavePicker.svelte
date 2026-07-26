<script lang="ts">
	import { tick } from 'svelte';
	import { savePickerVisible, saveFiles, currentSaveState } from '../stores/save';
	import { discoverSaveFiles, loadBranch, newSaveFile, newGame, createBranch, getSaveState, getWorldSnapshot, getMap, getNpcsHere } from '$lib/ipc';
	import {
		worldState,
		mapData,
		npcsHere,
		resetTimeRule,
		textLog,
	} from '../stores/game';
	import type { SaveFileInfo, SaveBranchDisplay } from '$lib/types';
	import { layoutTree } from '$lib/save-picker/dag';
	import LedgerList from '$lib/save-picker/LedgerList.svelte';
	import DagTree from '$lib/save-picker/DagTree.svelte';

	let loadingCount = $state(0);
	const loading = $derived(loadingCount > 0);
	let forkingBranchId: number | null = $state(null);
	let forkName = $state('');
	let forkError = $state('');
	let showLedgers = $state(false);
	let modalBodyEl: HTMLDivElement | undefined = $state();

	// ── Handlers ────────────────────────────────────────────────────

	/**
	 * Runs `fn` inside the shared loading-count guard (TD-040): increments
	 * `loadingCount` for the duration, logs any rejection under `label`, then
	 * decrements in `finally` so the spinner clears even on error. `onError`
	 * lets a caller record extra state (e.g. a user-visible message) on failure.
	 */
	async function withLoading(
		label: string,
		fn: () => Promise<void>,
		onError?: (e: unknown) => void
	): Promise<void> {
		loadingCount++;
		try {
			await fn();
		} catch (e) {
			console.error(`${label}:`, e);
			onError?.(e);
		} finally {
			loadingCount--;
		}
	}

	async function refreshSaves() {
		await withLoading('Failed to discover saves', async () => {
			const allFiles = await discoverSaveFiles();
			saveFiles.set(allFiles);
			const state = await getSaveState();
			currentSaveState.set(state);
		});
	}

	async function refreshGameState() {
		// A ledger/branch is a separate narrative context. Clear immediately
		// after the backend switch succeeds, before any refresh awaits, so a
		// partial refresh failure can never leave the previous branch's words
		// displayed under the new authoritative state.
		textLog.set([]);
		try {
			// New game / branch switch: re-prime time-rule tracking so the
			// incoming snapshot never emits a separator carried over from the
			// previous session's period (PR #1419 review).
			resetTimeRule();
			const [ws, md, npcs] = await Promise.all([
				getWorldSnapshot(),
				getMap(),
				getNpcsHere()
			]);
			worldState.set(ws);
			mapData.set(md);
			npcsHere.set(npcs);
			textLog.set(
				ws.location_description
					? [
							{
								source: 'system',
								subtype: 'location',
								content: ws.location_description,
							},
						]
					: [],
			);
		} catch (e) {
			console.error('Failed to refresh game state:', e);
		}
	}

	async function handleLoadBranch(file: SaveFileInfo, branch: SaveBranchDisplay) {
		await withLoading('Load failed', async () => {
			await loadBranch(file.path, branch.id);
			await refreshGameState();
			savePickerVisible.set(false);
		});
	}

	async function handleForkLedger() {
		await withLoading('Fork ledger failed', async () => {
			await newSaveFile();
			await refreshGameState();
			showLedgers = false;
			savePickerVisible.set(false);
		});
	}

	async function handleNewGame() {
		await withLoading('New game failed', async () => {
			await newGame();
			await refreshGameState();
			showLedgers = false;
			savePickerVisible.set(false);
		});
	}

	async function handleSwitchLedger(file: SaveFileInfo) {
		const branch = file.branches[0];
		if (!branch) return;
		await withLoading('Switch ledger failed', async () => {
			await loadBranch(file.path, branch.id);
			await refreshGameState();
			showLedgers = false;
			await refreshSaves();
		});
	}

	async function handleFork(parentBranch: SaveBranchDisplay) {
		const name = forkName.trim();
		if (!name) return;
		await withLoading(
			'Branch creation failed',
			async () => {
				await createBranch(name, parentBranch.id);
				forkingBranchId = null;
				forkName = '';
				const body = modalBodyEl;
				const scrollTop = body?.scrollTop ?? 0;
				const scrollLeft = body?.scrollLeft ?? 0;
				await refreshSaves();
				requestAnimationFrame(() => {
					if (body) {
						body.scrollTop = scrollTop;
						body.scrollLeft = scrollLeft;
					}
				});
			},
			(e) => {
				forkError = (e instanceof Error ? e.message : String(e)).substring(0, 60);
			}
		);
	}

	function generateBranchName(parent: SaveBranchDisplay, branches: SaveBranchDisplay[]): string {
		const existing = new Set(branches.map(b => b.name));
		if (parent.latest_location) {
			const locSlug = parent.latest_location.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '');
			if (!existing.has(locSlug)) return locSlug;
			for (let i = 2; i < 100; i++) {
				const name = `${locSlug}-${i}`;
				if (!existing.has(name)) return name;
			}
		}
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
		const current = modalBodyEl?.querySelector('.dag-current');
		if (current) {
			current.scrollIntoView({ behavior: 'instant', block: 'center', inline: 'center' });
		}
	}

	const files = $derived($saveFiles);
	const saveState = $derived($currentSaveState);
	const activeFile = $derived(files.find(f => f.filename === saveState?.filename) ?? files[0] ?? null);

	let prevVisible = $state(false);
	$effect(() => {
		const visible = $savePickerVisible;
		if (visible && !prevVisible) {
			refreshSaves().then(scrollToCurrentNode);
		}
		prevVisible = visible;
	});

	const PHANTOM_ID = -999;

	const layoutBranches = $derived.by(() => {
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
	});
	const layout = $derived(layoutBranches.length > 0 ? layoutTree(layoutBranches) : null);

	function clearForkError() { forkError = ''; }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $savePickerVisible}
	<div class="overlay" role="dialog" aria-modal="true" aria-label="The Parish Ledger" data-testid="save-picker">
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

			<div class="modal-body" bind:this={modalBodyEl}>
				{#if loading && files.length === 0}
					<div class="loading-msg">Scanning save files...</div>
				{/if}

				{#if showLedgers}
					<LedgerList {files} {saveState} {loading} onswitchledger={handleSwitchLedger} onforkledger={handleForkLedger} onnewgame={handleNewGame} />
				{:else if layout && activeFile}
					<DagTree {activeFile} {layout} {forkingBranchId} bind:forkName {forkError} {loading} {PHANTOM_ID} {modalBodyEl} currentBranchName={saveState?.branch_name ?? ''} onloadbranch={handleLoadBranch} onstartfork={startFork} oncancelfork={cancelFork} onfork={handleFork} onforkerrorclear={clearForkError} />
				{:else}
					<div class="loading-msg">No save file found.</div>
				{/if}
			</div>

			<div class="modal-footer">
				{#if showLedgers}
					<button class="footer-btn" onclick={() => { showLedgers = false; }}>
						← Back
					</button>
				{:else}
					<button class="footer-btn" onclick={() => { showLedgers = true; }}>
						Ledgers
					</button>
				{/if}
				<span class="footer-spacer"></span>
				<button class="footer-btn" onclick={close}>Close</button>
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

	.loading-msg {
		color: var(--color-muted);
		font-size: 0.8rem;
		font-style: italic;
		padding: 1rem 0;
		text-align: center;
	}
</style>
