<script lang="ts">
import {
	debugVisible,
	debugSnapshot,
	debugTab,
	debugDockLeft,
	selectedNpcId
} from '../stores/debug';
import DebugOverviewTab from './DebugOverviewTab.svelte';
import DebugNpcsTab from './DebugNpcsTab.svelte';
import DebugWorldTab from './DebugWorldTab.svelte';
import DebugWeatherTab from './DebugWeatherTab.svelte';
import DebugGossipTab from './DebugGossipTab.svelte';
import DebugConversationsTab from './DebugConversationsTab.svelte';
import DebugEventsTab from './DebugEventsTab.svelte';
import DebugInferenceTab from './DebugInferenceTab.svelte';

	const tabs = [
		'Overview',
		'NPCs',
		'World',
		'Weather',
		'Gossip',
		'Conv',
		'Events',
		'Inference'
	];

	let selectedLogId: number | null = $state(null);

	function selectTab(index: number) {
		debugTab.set(index);
		selectedNpcId.set(null);
		selectedLogId = null;
	}

	function closePanel() {
		selectedLogId = null;
		selectedNpcId.set(null);
		debugVisible.set(false);
	}

	function selectNpc(id: number) {
		selectedNpcId.set(id);
	}

	function deselectNpc() {
		selectedNpcId.set(null);
	}

	function selectLog(id: number) {
		selectedLogId = id;
	}

	function deselectLog() {
		selectedLogId = null;
	}

	const snap = $derived($debugSnapshot);
	const tab = $derived($debugTab);
	const _npcId = $derived($selectedNpcId);
</script>

{#if $debugVisible && snap}
	<div class="debug-panel" data-testid="debug-panel" class:left-dock={$debugDockLeft}>
		<div class="debug-header">
			<span class="debug-title">Debug</span>
			<div class="debug-actions">
				<button class="debug-dock-toggle" aria-label={$debugDockLeft ? 'Dock panel to bottom' : 'Dock panel to left'} onclick={() => debugDockLeft.update((v) => !v)}>
					{$debugDockLeft ? 'Dock Bottom' : 'Dock Left'}
				</button>
				<button class="debug-close" aria-label="Close debug panel" title="Close debug panel" onclick={closePanel}>X</button>
			</div>
		</div>

		<div class="tab-bar">
			{#each tabs as tabName, i (tabName)}
				<button
					class="tab-btn"
					class:active={tab === i}
					onclick={() => selectTab(i)}
				>
					{tabName}
				</button>
			{/each}
		</div>

		<div class="tab-content">
			{#if tab === 0}
				<DebugOverviewTab {snap} />
			{:else if tab === 1}
				<DebugNpcsTab {snap} npcId={$selectedNpcId} onSelectNpc={selectNpc} onDeselectNpc={deselectNpc} />
			{:else if tab === 2}
				<DebugWorldTab {snap} />
			{:else if tab === 3}
				<DebugWeatherTab {snap} />
			{:else if tab === 4}
				<DebugGossipTab {snap} />
			{:else if tab === 5}
				<DebugConversationsTab {snap} />
			{:else if tab === 6}
				<DebugEventsTab {snap} />
			{:else if tab === 7}
				<DebugInferenceTab {snap} logId={selectedLogId} onSelectLog={selectLog} onDeselectLog={deselectLog} />
			{/if}
		</div>
	</div>
{/if}

<style>
	.debug-panel {
		height: 40vh;
		background: var(--color-panel-bg);
		border-top: 2px solid var(--color-accent);
		display: flex;
		flex-direction: column;
		font-size: 0.75rem;
		overflow: hidden;
	}

	.debug-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.25rem 0.75rem;
		background: var(--color-bg);
		border-bottom: 1px solid var(--color-border);
	}

	.debug-title {
		color: var(--color-accent);
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		font-size: 0.7rem;
	}

	.debug-actions {
		display: flex;
		gap: 0.4rem;
		align-items: center;
	}

	.debug-dock-toggle,
	.debug-close {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		padding: 0.1rem 0.4rem;
		font-size: 0.65rem;
	}

	.debug-dock-toggle:hover,
	.debug-dock-toggle:focus-visible,
	.debug-close:hover,
	.debug-close:focus-visible {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	@media (min-width: 1200px) {
		.debug-panel.left-dock {
			position: fixed;
			left: 0;
			top: 0;
			height: 100dvh;
			width: min(28rem, 36vw);
			border-top: none;
			border-right: 2px solid var(--color-accent);
			z-index: 45;
		}
	}

	.tab-bar {
		display: flex;
		gap: 0;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg);
		overflow-x: auto;
	}

	.tab-btn {
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--color-muted);
		padding: 0.35rem 0.75rem;
		font-size: 0.7rem;
		cursor: pointer;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.tab-btn:hover,
	.tab-btn:focus-visible {
		color: var(--color-fg);
	}

	.tab-btn.active {
		color: var(--color-accent);
		border-bottom-color: var(--color-accent);
	}

	.tab-content {
		flex: 1;
		overflow-y: auto;
		padding: 0.5rem 0.75rem;
	}
</style>
