<script lang="ts">
	import { debugVisible, debugSnapshot, debugTab, selectedNpcId } from '../stores/debug';
	import type { NpcDebug } from '$lib/types';

	const tabs = ['Overview', 'NPCs', 'World', 'Events', 'Inference'];

	function selectTab(index: number) {
		debugTab.set(index);
		selectedNpcId.set(null);
	}

	function selectNpc(id: number) {
		selectedNpcId.set(id);
	}

	function deselectNpc() {
		selectedNpcId.set(null);
	}

	function strengthBar(strength: number): string {
		const normalized = Math.round(((strength + 1) / 2) * 10);
		const filled = Math.min(normalized, 10);
		const empty = 10 - filled;
		return '[' + '#'.repeat(filled) + '.'.repeat(empty) + ']';
	}

	$: snap = $debugSnapshot;
	$: tab = $debugTab;
	$: npcId = $selectedNpcId;
	$: selectedNpc = snap?.npcs.find((n: NpcDebug) => n.id === npcId) ?? null;
</script>

{#if $debugVisible && snap}
	<div class="debug-panel">
		<div class="debug-header">
			<span class="debug-title">Debug</span>
			<button class="debug-close" on:click={() => debugVisible.set(false)}>X</button>
		</div>

		<div class="tab-bar">
			{#each tabs as tabName, i}
				<button
					class="tab-btn"
					class:active={tab === i}
					on:click={() => selectTab(i)}
				>
					{tabName}
				</button>
			{/each}
		</div>

		<div class="tab-content">
			{#if tab === 0}
				<!-- Overview -->
				<div class="section">
					<h4>Clock</h4>
					<div class="field">{snap.clock.game_time}</div>
					<div class="field">{snap.clock.time_of_day} | {snap.clock.season}</div>
					<div class="field">Weather: {snap.clock.weather}</div>
					<div class="field">Speed: {snap.clock.speed_factor}x {snap.clock.paused ? '(PAUSED)' : ''}</div>
					{#if snap.clock.festival}
						<div class="field accent">Festival: {snap.clock.festival}</div>
					{/if}
				</div>
				<div class="section">
					<h4>Location</h4>
					<div class="field accent"># {snap.world.player_location_name}</div>
					<div class="field muted">{snap.world.location_count} locations total</div>
				</div>
				<div class="section">
					<h4>Tiers</h4>
					<div class="field">T1: {snap.tier_summary.tier1_count} | T2: {snap.tier_summary.tier2_count} | T3: {snap.tier_summary.tier3_count} | T4: {snap.tier_summary.tier4_count}</div>
					{#if snap.tier_summary.tier1_names.length > 0}
						<div class="field muted">T1 NPCs: {snap.tier_summary.tier1_names.join(', ')}</div>
					{/if}
				</div>

			{:else if tab === 1}
				<!-- NPCs -->
				{#if selectedNpc}
					<div class="npc-detail">
						<button class="back-btn" on:click={deselectNpc}>Back to list</button>
						<h4 class="accent">{selectedNpc.name}</h4>

						<div class="section">
							<h5>Identity</h5>
							<div class="field">Age: {selectedNpc.age} | {selectedNpc.occupation}</div>
							<div class="field muted">{selectedNpc.personality.length > 120 ? selectedNpc.personality.slice(0, 117) + '...' : selectedNpc.personality}</div>
						</div>

						<div class="section">
							<h5>Location</h5>
							<div class="field">Current: {selectedNpc.location_name}</div>
							{#if selectedNpc.home_name}<div class="field">Home: {selectedNpc.home_name}</div>{/if}
							{#if selectedNpc.workplace_name}<div class="field">Work: {selectedNpc.workplace_name}</div>{/if}
						</div>

						<div class="section">
							<h5>Status</h5>
							<div class="field">Mood: {selectedNpc.mood}</div>
							<div class="field">Tier: {selectedNpc.tier} | {selectedNpc.state}</div>
						</div>

						<div class="section">
							<h5>Intelligence</h5>
							<div class="field">Verbal: {selectedNpc.intelligence.verbal} | Analytical: {selectedNpc.intelligence.analytical} | Emotional: {selectedNpc.intelligence.emotional} | Practical: {selectedNpc.intelligence.practical} | Wisdom: {selectedNpc.intelligence.wisdom} | Creative: {selectedNpc.intelligence.creative}</div>
						</div>

						{#if selectedNpc.schedule.length > 0}
							<div class="section">
								<h5>Schedule</h5>
								{#each selectedNpc.schedule as entry}
									<div class="field">{String(entry.start_hour).padStart(2, '0')}:00-{String(entry.end_hour).padStart(2, '0')}:00 {entry.location_name} ({entry.activity})</div>
								{/each}
							</div>
						{/if}

						{#if selectedNpc.relationships.length > 0}
							<div class="section">
								<h5>Relationships</h5>
								{#each selectedNpc.relationships as rel}
									<div class="field"><span class="mono">{strengthBar(rel.strength)}</span> {rel.target_name} ({rel.kind}, {rel.strength.toFixed(1)})</div>
								{/each}
							</div>
						{/if}

						{#if selectedNpc.memories.length > 0}
							<div class="section">
								<h5>Memory ({selectedNpc.memories.length})</h5>
								{#each selectedNpc.memories as mem}
									<div class="field"><span class="muted">[{mem.timestamp}]</span> {mem.content} <span class="muted">({mem.location_name})</span></div>
								{/each}
							</div>
						{/if}

						{#if selectedNpc.knowledge.length > 0}
							<div class="section">
								<h5>Knowledge</h5>
								{#each selectedNpc.knowledge as item}
									<div class="field">- {item}</div>
								{/each}
							</div>
						{/if}
					</div>
				{:else}
					<div class="npc-list">
						{#each snap.npcs as npc}
							<button class="npc-row" on:click={() => selectNpc(npc.id)}>
								<span class="npc-name">{npc.name}</span>
								<span class="npc-tier">[{npc.tier}]</span>
								<span class="npc-mood">{npc.mood}</span>
								<span class="npc-loc muted"># {npc.location_name}</span>
								{#if npc.state !== 'Present'}
									<span class="npc-state muted">{npc.state}</span>
								{/if}
							</button>
						{/each}
						{#if snap.npcs.length === 0}
							<div class="field muted">(no NPCs)</div>
						{/if}
					</div>
				{/if}

			{:else if tab === 2}
				<!-- World -->
				<div class="section">
					<h4>{snap.world.location_count} Locations</h4>
					{#each snap.world.locations as loc}
						<div class="loc-row" class:player-here={loc.id === snap.world.player_location_id}>
							<div class="field">
								{#if loc.id === snap.world.player_location_id}<strong>>>> </strong>{/if}
								{loc.name}
								<span class="muted">({loc.indoor ? 'indoor' : 'outdoor'}/{loc.public ? 'pub' : 'prv'}, {loc.connection_count} exits)</span>
							</div>
							{#if loc.npcs_here.length > 0}
								<div class="field muted indent">NPCs: {loc.npcs_here.join(', ')}</div>
							{/if}
						</div>
					{/each}
				</div>

			{:else if tab === 3}
				<!-- Events -->
				<div class="section">
					<h4>Events ({snap.events.length})</h4>
					{#if snap.events.length === 0}
						<div class="field muted">(no events yet)</div>
					{:else}
						{#each [...snap.events].reverse() as evt}
							<div class="field"><span class="muted">[{evt.timestamp}]</span> <span class="event-cat">[{evt.category}]</span> {evt.message}</div>
						{/each}
					{/if}
				</div>

			{:else if tab === 4}
				<!-- Inference -->
				<div class="section">
					<h4>Base Provider</h4>
					<div class="field">Provider: {snap.inference.provider_name}</div>
					<div class="field">Model: {snap.inference.model_name || '(auto)'}</div>
					<div class="field">URL: {snap.inference.base_url || '(default)'}</div>
					<div class="field">Queue: {snap.inference.has_queue ? 'active' : 'none'}</div>
				</div>
				<div class="section">
					<h4>Cloud</h4>
					{#if snap.inference.cloud_provider}
						<div class="field">Provider: {snap.inference.cloud_provider}</div>
						<div class="field">Model: {snap.inference.cloud_model || '(none)'}</div>
					{:else}
						<div class="field muted">(not configured)</div>
					{/if}
				</div>
				<div class="section">
					<div class="field">Improv: {snap.inference.improv_enabled ? 'ON' : 'OFF'}</div>
				</div>
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

	.debug-close {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		padding: 0.1rem 0.4rem;
		font-size: 0.65rem;
	}

	.debug-close:hover {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	.tab-bar {
		display: flex;
		gap: 0;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg);
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

	.tab-btn:hover {
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

	.section {
		margin-bottom: 0.75rem;
	}

	h4 {
		color: var(--color-accent);
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		margin: 0 0 0.25rem;
	}

	h5 {
		color: var(--color-accent);
		font-size: 0.65rem;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		margin: 0.5rem 0 0.15rem;
	}

	.field {
		color: var(--color-fg);
		line-height: 1.4;
		word-break: break-word;
	}

	.accent {
		color: var(--color-accent);
	}

	.muted {
		color: var(--color-muted);
	}

	.mono {
		font-family: monospace;
		font-size: 0.7rem;
	}

	.indent {
		padding-left: 1rem;
	}

	.npc-list {
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	.npc-row {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: baseline;
		padding: 0.3rem 0.5rem;
		background: none;
		border: none;
		border-bottom: 1px solid var(--color-border);
		cursor: pointer;
		text-align: left;
		font-size: 0.75rem;
		color: var(--color-fg);
	}

	.npc-row:hover {
		background: var(--color-input-bg);
	}

	.npc-name {
		color: var(--color-accent);
		font-weight: 600;
	}

	.npc-tier {
		color: var(--color-muted);
		font-size: 0.65rem;
	}

	.npc-mood {
		color: var(--color-fg);
	}

	.npc-loc {
		font-size: 0.65rem;
	}

	.npc-state {
		font-size: 0.65rem;
		font-style: italic;
	}

	.npc-detail {
		display: flex;
		flex-direction: column;
	}

	.back-btn {
		align-self: flex-start;
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		padding: 0.15rem 0.5rem;
		font-size: 0.65rem;
		margin-bottom: 0.5rem;
	}

	.back-btn:hover {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	.player-here {
		background: color-mix(in srgb, var(--color-accent) 8%, transparent);
	}

	.loc-row {
		padding: 0.2rem 0;
		border-bottom: 1px solid var(--color-border);
	}

	.event-cat {
		color: var(--color-accent);
		font-size: 0.65rem;
	}
</style>
