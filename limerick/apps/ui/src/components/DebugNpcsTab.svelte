<script lang="ts">
	import type { DebugSnapshot, NpcDebug } from '$lib/types';

	let { snap, npcId, onSelectNpc, onDeselectNpc }: {
		snap: DebugSnapshot;
		npcId: number | null;
		onSelectNpc: (id: number) => void;
		onDeselectNpc: () => void;
	} = $props();

	const selectedNpc = $derived(snap.npcs.find((n: NpcDebug) => n.id === npcId) ?? null);

	function strengthBar(strength: number): string {
		const normalized = Math.round(((strength + 1) / 2) * 10);
		const filled = Math.min(normalized, 10);
		const empty = 10 - filled;
		return '[' + '#'.repeat(filled) + '.'.repeat(empty) + ']';
	}
</script>

{#if selectedNpc}
	<div class="npc-detail">
		<button class="back-btn" onclick={onDeselectNpc}>Back to list</button>
		<h4 class="accent">{selectedNpc.name}</h4>

		<div class="section">
			<h5>Identity</h5>
			<div class="field">Age: {selectedNpc.age} | {selectedNpc.occupation}</div>
			<div class="field muted">{selectedNpc.personality.length > 120 ? selectedNpc.personality.slice(0, 117) + '...' : selectedNpc.personality}</div>
			<div class="field muted">Brief: {selectedNpc.brief_description}</div>
			<div class="field">
				Introduced: {selectedNpc.introduced ? 'yes' : 'no'}
				{#if selectedNpc.is_ill}<span class="accent"> ILL</span>{/if}
			</div>
		</div>

		<div class="section">
			<h5>Location</h5>
			<div class="field">Current: {selectedNpc.location_name}</div>
			{#if selectedNpc.home_name}<div class="field">Home: {selectedNpc.home_name}</div>{/if}
			{#if selectedNpc.workplace_name}<div class="field">Work: {selectedNpc.workplace_name}</div>{/if}
		</div>

		<div class="section">
			<h5>Status</h5>
			<div class="field">
				Mood: {selectedNpc.mood}
				{#if selectedNpc.is_ill}
					<span class="accent"> Ill</span>
				{/if}
			</div>
			<div class="field">Tier: {selectedNpc.tier} | {selectedNpc.state}</div>
			<div class="field">Knows player name: {#if selectedNpc.knows_player_name}<span class="accent">yes</span>{:else}<span class="muted">no</span>{/if}</div>
		</div>

		{#if selectedNpc.last_activity}
			<div class="section">
				<h5>Last Batch Activity</h5>
				<div class="field muted">{selectedNpc.last_activity}</div>
			</div>
		{/if}

		<div class="section">
			<h5>Intelligence</h5>
			<div class="field">Verbal: {selectedNpc.intelligence.verbal} | Analytical: {selectedNpc.intelligence.analytical} | Emotional: {selectedNpc.intelligence.emotional} | Practical: {selectedNpc.intelligence.practical} | Wisdom: {selectedNpc.intelligence.wisdom} | Creative: {selectedNpc.intelligence.creative}</div>
		</div>

		{#if selectedNpc.schedule.length > 0}
			<div class="section">
				<h5>Schedule</h5>
				{#each selectedNpc.schedule as variant, vi (vi)}
					{@const variantLabel = [variant.season ?? 'All seasons', variant.day_type ?? 'All days'].join(' · ')}
					<div class="schedule-variant" class:variant-active={variant.is_active}>
						<div class="variant-label">
							{variantLabel}
							{#if variant.is_active}<span class="active-badge">ACTIVE</span>{/if}
						</div>
						{#each variant.entries as entry, ei (ei)}
							<div class="schedule-entry" class:entry-current={entry.is_current}>
								{String(entry.start_hour).padStart(2, '0')}:00\u2013{String(entry.end_hour).padStart(2, '0')}:00
								{entry.location_name}
								<span class="muted">({entry.activity})</span>
								{#if entry.is_current}<span class="now-badge">NOW</span>{/if}
							</div>
						{/each}
					</div>
				{/each}
			</div>
		{/if}

		{#if selectedNpc.relationships.length > 0}
			<div class="section">
				<h5>Relationships</h5>
				{#each selectedNpc.relationships as rel (rel.target_name)}
					<div class="field"><span class="mono">{strengthBar(rel.strength)}</span> {rel.target_name} ({rel.kind}, {rel.strength.toFixed(1)}, {rel.history_count} events)</div>
					{#if rel.history.length > 0}
						{#each rel.history as evt, hi (hi)}
							<div class="field indent muted">[{evt.timestamp}] {evt.description}</div>
						{/each}
					{/if}
				{/each}
			</div>
		{/if}

		{#if selectedNpc.memories.length > 0 || selectedNpc.long_term_memories.length > 0}
			<div class="section">
				<h5>Short-term Memory ({selectedNpc.memories.length})</h5>
				{#each selectedNpc.memories as mem, mi (mi)}
					<div class="field"><span class="muted">[{mem.timestamp}]</span> {mem.content} <span class="muted">({mem.location_name})</span></div>
				{/each}
			</div>
		{/if}

		{#if selectedNpc.long_term_memories.length > 0}
			<div class="section">
				<h5>Long-term Memory ({selectedNpc.long_term_memories.length})</h5>
				{#each selectedNpc.long_term_memories as ltm, li (li)}
					<div class="field"><span class="muted">[{ltm.timestamp}]</span> ({ltm.importance.toFixed(2)}) {ltm.content}</div>
					{#if ltm.keywords.length > 0}
						<div class="field indent muted">kw: {ltm.keywords.join(', ')}</div>
					{/if}
				{/each}
			</div>
		{/if}

		{#if selectedNpc.reactions.length > 0}
			<div class="section">
				<h5>Reactions ({selectedNpc.reactions.length})</h5>
				{#each selectedNpc.reactions as r, ri (ri)}
					<div class="field"><span class="muted">[{r.timestamp}]</span> {r.emoji} {r.direction === 'PlayerToNpc' ? 'Player' : selectedNpc.name}: {r.description}</div>
					<div class="field indent muted">context: {r.context}</div>
				{/each}
			</div>
		{/if}

		{#if selectedNpc.deflated_summary}
			<div class="section">
				<h5>Deflated Summary</h5>
				<div class="field">@ {selectedNpc.deflated_summary.location_name} \u2014 {selectedNpc.deflated_summary.mood}</div>
				{#each selectedNpc.deflated_summary.recent_activity as act, ai (ai)}
					<div class="field indent muted">- {act}</div>
				{/each}
				{#each selectedNpc.deflated_summary.key_relationship_changes as ch, ci (ci)}
					<div class="field indent muted">~ {ch}</div>
				{/each}
			</div>
		{/if}

		{#if selectedNpc.knowledge.length > 0}
			<div class="section">
				<h5>Knowledge</h5>
				{#each selectedNpc.knowledge as item, ki (ki)}
					<div class="field">- {item}</div>
				{/each}
			</div>
		{/if}
	</div>
{:else}
	<div class="npc-list">
		{#each snap.npcs as npc (npc.id)}
			<button class="npc-row" onclick={() => onSelectNpc(npc.id)}>
				<span class="npc-name">{npc.name}</span>
				<span class="npc-tier">[{npc.tier}]</span>
				<span class="npc-mood">{npc.mood}</span>
				<span class="npc-loc muted"># {npc.location_name}</span>
				{#if npc.knows_player_name}
					<span class="npc-named accent">[named]</span>
				{/if}
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

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.accent { color: var(--color-accent); }
	.muted { color: var(--color-muted); }
	.indent { padding-left: 1rem; }
	.mono { font-family: monospace; font-size: 0.7rem; }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }
	h5 { color: var(--color-accent); font-size: 0.65rem; text-transform: uppercase; letter-spacing: 0.06em; margin: 0.5rem 0 0.15rem; }

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

	.back-btn:hover,
	.back-btn:focus-visible {
		color: var(--color-fg);
		border-color: var(--color-accent);
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

	.schedule-variant {
		margin-bottom: 0.4rem;
		border-left: 2px solid var(--color-border);
		padding-left: 0.4rem;
	}

	.schedule-variant.variant-active {
		border-left-color: var(--color-accent);
	}

	.variant-label {
		font-size: 0.65rem;
		color: var(--color-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-bottom: 0.15rem;
		display: flex;
		align-items: center;
		gap: 0.4rem;
	}

	.schedule-variant.variant-active .variant-label {
		color: var(--color-accent);
	}

	.active-badge {
		font-size: 0.55rem;
		padding: 0.05rem 0.25rem;
		background: color-mix(in srgb, var(--color-accent) 20%, transparent);
		color: var(--color-accent);
		border-radius: 2px;
		font-weight: 700;
	}

	.schedule-entry {
		font-size: 0.72rem;
		line-height: 1.4;
		color: var(--color-fg);
		padding: 0.05rem 0;
	}

	.schedule-entry.entry-current {
		color: var(--color-accent);
		font-weight: 600;
	}

	.now-badge {
		font-size: 0.55rem;
		padding: 0.05rem 0.25rem;
		background: color-mix(in srgb, #44cc44 20%, transparent);
		color: #44cc44;
		border-radius: 2px;
		font-weight: 700;
		margin-left: 0.25rem;
	}

	.npc-named {
		font-size: 0.65rem;
	}
</style>
