<script lang="ts">
	import { type LayoutResult, NODE_W, NODE_H, GAP_Y } from './dag';
	import type { SaveFileInfo, SaveBranchDisplay } from '$lib/types';

	let {
		activeFile = null as SaveFileInfo | null,
		layout = null as LayoutResult | null,
		forkingBranchId = null as number | null,
		forkName = $bindable(''),
		forkError = '',
		loading = false,
		PHANTOM_ID = -999,
		modalBodyEl = undefined as HTMLDivElement | undefined,
		currentBranchName = '',
		onloadbranch = (_file: SaveFileInfo, _branch: SaveBranchDisplay) => {},
		onstartfork = (_branchId: number) => {},
		oncancelfork = () => {},
		onfork = (_parent: SaveBranchDisplay) => {},
		onforkerrorclear = () => {}
	} = $props();

	function autofocus(node: HTMLInputElement) {
		node.focus();
		node.select();
		requestAnimationFrame(() => {
			const dagNode = node.closest('.dag-node') as HTMLElement | null;
			const body = modalBodyEl;
			if (dagNode && body) {
				const nodeRect = dagNode.getBoundingClientRect();
				const bodyRect = body.getBoundingClientRect();
				const scrollPad = 30;
				if (nodeRect.top < bodyRect.top + scrollPad) {
					body.scrollTop -= (bodyRect.top + scrollPad - nodeRect.top);
				}
				if (nodeRect.bottom > bodyRect.bottom - scrollPad) {
					body.scrollTop += (nodeRect.bottom - bodyRect.bottom + scrollPad);
				}
				if (nodeRect.right > bodyRect.right - scrollPad) {
					body.scrollLeft += (nodeRect.right - bodyRect.right + scrollPad);
				}
			}
		});
	}
</script>

<style>
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
</style>

{#if layout && activeFile}
	<div class="dag-scroll">
		<div class="dag-container" style="width: {layout.width}px; height: {layout.height}px;">
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

			{#each layout.nodes as node (node.branch.id)}
				{#if node.branch.id === PHANTOM_ID}
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
								onkeydown={(e) => { e.stopPropagation(); if (e.key === 'Enter' && parent) { e.preventDefault(); onfork(parent); } if (e.key === 'Escape') oncancelfork(); }}
								oninput={() => { onforkerrorclear(); }}
							/>
							{#if forkError}
								<span class="fork-error">{forkError}</span>
							{:else}
								<span class="node-location">{node.branch.latest_location ?? 'New'}</span>
							{/if}
							<div class="phantom-actions">
								<button class="phantom-btn" onclick={(e) => { e.stopPropagation(); if (parent) onfork(parent); }} disabled={loading || !forkName.trim()}>Create</button>
								<button class="phantom-btn" onclick={(e) => { e.stopPropagation(); oncancelfork(); }}>Cancel</button>
							</div>
						</div>
					</div>
				{:else}
					{@const isCurrent = node.branch.name === currentBranchName}
					<div
						class="dag-node"
						class:dag-current={isCurrent}
						style="left: {node.x}px; top: {node.y}px; width: {NODE_W}px; height: {NODE_H}px;"
					>
						<button
							class="node-body"
							disabled={loading}
							onclick={() => onloadbranch(activeFile, node.branch)}
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
							onclick={(e) => { e.stopPropagation(); onstartfork(node.branch.id); }}
						>Branch From Here</button>
					</div>
				{/if}
			{/each}
		</div>
	</div>
{/if}
