<script lang="ts">
	import { debugVisible, debugSnapshot, debugTab, selectedNpcId } from '../stores/debug';
	import type { NpcDebug, ScheduleVariantDebug } from '$lib/types';

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

	function selectTab(index: number) {
		debugTab.set(index);
		selectedNpcId.set(null);
		selectedLogId = null;
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

	let selectedLogId: number | null = null;

	function selectLogEntry(id: number) {
		selectedLogId = id;
	}

	function deselectLogEntry() {
		selectedLogId = null;
	}

	$: snap = $debugSnapshot;
	$: tab = $debugTab;
	$: npcId = $selectedNpcId;
	$: selectedNpc = snap?.npcs.find((n: NpcDebug) => n.id === npcId) ?? null;
	$: selectedLog = snap?.inference.call_log.find(e => e.request_id === selectedLogId) ?? null;
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
					<div class="field">{snap.clock.time_of_day} | {snap.clock.day_of_week} | {snap.clock.season}</div>
					<div class="field muted">Schedule day: {snap.clock.day_type}</div>
					<div class="field">Weather: {snap.clock.weather}</div>
					<div class="field">
						Speed: {snap.clock.speed_factor}x
						{#if snap.clock.speed_name}<span class="muted">({snap.clock.speed_name})</span>{/if}
						{#if snap.clock.paused}<span class="accent"> PAUSED</span>{/if}
						{#if snap.clock.inference_paused}<span class="accent"> INFER-PAUSED</span>{/if}
					</div>
					{#if snap.clock.festival}
						<div class="field accent">Festival: {snap.clock.festival}</div>
					{/if}
					<div class="field muted">Anchor: {snap.clock.start_game_time}</div>
					{#if snap.clock.paused}
						<div class="field muted">Frozen at: {snap.clock.paused_game_time}</div>
					{/if}
					<div class="field muted">Real elapsed: {snap.clock.real_elapsed_secs.toFixed(1)}s</div>
				</div>
				<div class="section">
					<h4>Location</h4>
					<div class="field accent"># {snap.world.player_location_name}</div>
					<div class="field muted">{snap.world.visited_count}/{snap.world.location_count} visited</div>
				</div>
				<div class="section">
					<h4>Tiers</h4>
					<div class="field">T1: {snap.tier_summary.tier1_count} | T2: {snap.tier_summary.tier2_count} | T3: {snap.tier_summary.tier3_count} | T4: {snap.tier_summary.tier4_count}</div>
					{#if snap.tier_summary.tier1_names.length > 0}
						<div class="field muted">T1: {snap.tier_summary.tier1_names.join(', ')}</div>
					{/if}
					{#if snap.tier_summary.tier2_names.length > 0}
						<div class="field muted">T2: {snap.tier_summary.tier2_names.join(', ')}</div>
					{/if}
					{#if snap.tier_summary.tier3_names.length > 0}
						<div class="field muted">T3: {snap.tier_summary.tier3_names.join(', ')}</div>
					{/if}
					{#if snap.tier_summary.tier4_names.length > 0}
						<div class="field muted">T4: {snap.tier_summary.tier4_names.join(', ')}</div>
					{/if}
					<div class="field muted">Introduced: {snap.tier_summary.introduced_count}</div>
					<div class="field">T3 batch:
						{#if snap.tier_summary.tier3_in_flight}
							<span class="accent">IN FLIGHT</span>
						{:else}
							idle
						{/if}
						{#if snap.tier_summary.last_tier3_tick}
							| last: {snap.tier_summary.last_tier3_tick}
						{:else}
							| (never run)
						{/if}
					</div>
					<div class="field muted">
						T2 last: {snap.tier_summary.last_tier2_tick ?? '(never)'}
						| T4 last: {snap.tier_summary.last_tier4_tick ?? '(never)'}
					</div>
				</div>
				<div class="section">
					<h4>Event Bus</h4>
					<div class="field muted">
						Subscribers: {snap.event_bus.subscriber_count}
						| Captured: {snap.event_bus.recent_events.length}
					</div>
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
							<div class="field">Mood: {selectedNpc.mood}</div>
							<div class="field">Tier: {selectedNpc.tier} | {selectedNpc.state}</div>
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
								{#each selectedNpc.schedule as variant}
									{@const variantLabel = [variant.season ?? 'All seasons', variant.day_type ?? 'All days'].join(' · ')}
									<div class="schedule-variant" class:variant-active={variant.is_active}>
										<div class="variant-label">
											{variantLabel}
											{#if variant.is_active}<span class="active-badge">ACTIVE</span>{/if}
										</div>
										{#each variant.entries as entry}
											<div class="schedule-entry" class:entry-current={entry.is_current}>
												{String(entry.start_hour).padStart(2, '0')}:00–{String(entry.end_hour).padStart(2, '0')}:00
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
								{#each selectedNpc.relationships as rel}
									<div class="field"><span class="mono">{strengthBar(rel.strength)}</span> {rel.target_name} ({rel.kind}, {rel.strength.toFixed(1)}, {rel.history_count} events)</div>
									{#if rel.history.length > 0}
										{#each rel.history as evt}
											<div class="field indent muted">[{evt.timestamp}] {evt.description}</div>
										{/each}
									{/if}
								{/each}
							</div>
						{/if}

						{#if selectedNpc.memories.length > 0}
							<div class="section">
								<h5>Short-term Memory ({selectedNpc.memories.length})</h5>
								{#each selectedNpc.memories as mem}
									<div class="field"><span class="muted">[{mem.timestamp}]</span> {mem.content} <span class="muted">({mem.location_name})</span></div>
								{/each}
							</div>
						{/if}

						{#if selectedNpc.long_term_memories.length > 0}
							<div class="section">
								<h5>Long-term Memory ({selectedNpc.long_term_memories.length})</h5>
								{#each selectedNpc.long_term_memories as ltm}
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
								{#each selectedNpc.reactions as r}
									<div class="field"><span class="muted">[{r.timestamp}]</span> {r.emoji} {r.description}</div>
									<div class="field indent muted">context: {r.context}</div>
								{/each}
							</div>
						{/if}

						{#if selectedNpc.deflated_summary}
							<div class="section">
								<h5>Deflated Summary</h5>
								<div class="field">@ {selectedNpc.deflated_summary.location_name} — {selectedNpc.deflated_summary.mood}</div>
								{#each selectedNpc.deflated_summary.recent_activity as act}
									<div class="field indent muted">- {act}</div>
								{/each}
								{#each selectedNpc.deflated_summary.key_relationship_changes as ch}
									<div class="field indent muted">~ {ch}</div>
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
					<h4>Locations ({snap.world.visited_count}/{snap.world.location_count} visited)</h4>
					{#each snap.world.locations as loc}
						<div class="loc-row" class:player-here={loc.id === snap.world.player_location_id}>
							<div class="field">
								{#if loc.id === snap.world.player_location_id}<strong>>>> </strong>{/if}
								{loc.name}
								<span class="muted">({loc.indoor ? 'indoor' : 'outdoor'}/{loc.public ? 'pub' : 'prv'}, {loc.connection_count} exits{#if !loc.visited}, unvisited{/if})</span>
							</div>
							{#if loc.npcs_here.length > 0}
								<div class="field muted indent">NPCs: {loc.npcs_here.join(', ')}</div>
							{/if}
							{#if loc.edges.length > 0}
								{#each loc.edges as edge}
									<div class="field muted indent">→ {edge.target_name} ({edge.walking_minutes}m walk) — {edge.path_description}</div>
								{/each}
							{/if}
						</div>
					{/each}
				</div>
				{#if snap.world.edge_traversals.length > 0}
					<div class="section">
						<h4>Worn Paths (top edges)</h4>
						{#each snap.world.edge_traversals.slice(0, 20) as edge}
							<div class="field">{edge.from_name} ↔ {edge.to_name} <span class="muted">×{edge.count}</span></div>
						{/each}
					</div>
				{/if}
				<div class="section">
					<h4>Text Log (tail {snap.world.text_log_tail.length}/{snap.world.text_log_len})</h4>
					{#if snap.world.text_log_tail.length === 0}
						<div class="field muted">(empty)</div>
					{:else}
						{#each snap.world.text_log_tail as line}
							<div class="field muted">{line}</div>
						{/each}
					{/if}
				</div>

			{:else if tab === 3}
				<!-- Weather -->
				<div class="section">
					<h4>Weather Engine</h4>
					<div class="field">Current: <span class="accent">{snap.weather.current}</span></div>
					<div class="field">Since: {snap.weather.since}</div>
					<div class="field">Duration: {snap.weather.duration_hours.toFixed(2)}h</div>
					<div class="field muted">Min duration before next transition: {snap.weather.min_duration_hours}h</div>
					<div class="field muted">Last check hour: {snap.weather.last_check_hour ?? '(never)'}</div>
				</div>

			{:else if tab === 4}
				<!-- Gossip -->
				<div class="section">
					<h4>Gossip Network ({snap.gossip.item_count})</h4>
					{#if snap.gossip.items.length === 0}
						<div class="field muted">(no gossip)</div>
					{:else}
						{#each snap.gossip.items as item}
							<div class="gossip-item">
								<div class="field">
									<span class="muted">#{item.id}</span>
									{#if item.distortion_level > 0}
										<span class="accent">[distortion {item.distortion_level}]</span>
									{/if}
									{item.content}
								</div>
								<div class="field muted indent">source: {item.source_name} | known by {item.known_by.length}: {item.known_by.join(', ')}</div>
								<div class="field muted indent">at {item.timestamp}</div>
							</div>
						{/each}
					{/if}
				</div>

			{:else if tab === 5}
				<!-- Conversations -->
				<div class="section">
					<h4>Conversation Log ({snap.conversations.exchange_count})</h4>
					{#if snap.conversations.exchanges.length === 0}
						<div class="field muted">(no exchanges)</div>
					{:else}
						{#each [...snap.conversations.exchanges].reverse() as ex}
							<div class="conv-entry">
								<div class="field muted">[{ex.timestamp}] @ {ex.location_name}</div>
								<div class="field">Player: {ex.player_input}</div>
								<div class="field accent">{ex.speaker_name}: {ex.npc_dialogue}</div>
							</div>
						{/each}
					{/if}
				</div>

			{:else if tab === 6}
				<!-- Events -->
				<div class="section">
					<h4>Game Events ({snap.event_bus.recent_events.length}) — subscribers: {snap.event_bus.subscriber_count}</h4>
					{#if snap.event_bus.recent_events.length === 0}
						<div class="field muted">(no game events captured)</div>
					{:else}
						{#each [...snap.event_bus.recent_events].reverse() as evt}
							<div class="field"><span class="muted">[{evt.timestamp}]</span> <span class="event-cat">[{evt.kind}]</span> {evt.summary}</div>
						{/each}
					{/if}
				</div>
				<div class="section">
					<h4>Debug Events ({snap.events.length})</h4>
					{#if snap.events.length === 0}
						<div class="field muted">(no events yet)</div>
					{:else}
						{#each [...snap.events].reverse() as evt}
							<div class="field"><span class="muted">[{evt.timestamp}]</span> <span class="event-cat">[{evt.category}]</span> {evt.message}</div>
						{/each}
					{/if}
				</div>

			{:else if tab === 7}
				<!-- Inference -->
				{#if selectedLog}
					<!-- Detail view for a single inference call -->
					<div class="npc-detail">
						<button class="back-btn" on:click={deselectLogEntry}>Back to list</button>
						<h4>Request #{selectedLog.request_id}</h4>

						<div class="section">
							<h5>Summary</h5>
							<div class="field">Time: {selectedLog.timestamp}</div>
							<div class="field">Model: <span class="accent">{selectedLog.model}</span></div>
							<div class="field">Duration: {selectedLog.duration_ms}ms</div>
							<div class="field">Streaming: {selectedLog.streaming ? 'yes' : 'no'}</div>
							{#if selectedLog.max_tokens}
								<div class="field">Max tokens: {selectedLog.max_tokens}</div>
							{/if}
							<div class="field">Status:
								{#if selectedLog.error}<span class="log-badge error">ERROR</span>{:else}<span class="log-badge ok">OK</span>{/if}
							</div>
						</div>

						{#if selectedLog.system_prompt}
							<div class="section">
								<h5>System Prompt ({selectedLog.system_prompt.length}ch)</h5>
								<pre class="prompt-block">{selectedLog.system_prompt}</pre>
							</div>
						{/if}

						<div class="section">
							<h5>Prompt ({selectedLog.prompt_len}ch)</h5>
							<pre class="prompt-block">{selectedLog.prompt_text}</pre>
						</div>

						<div class="section">
							<h5>Response ({selectedLog.response_len}ch)</h5>
							{#if selectedLog.error}
								<div class="log-error-msg">{selectedLog.error}</div>
							{:else}
								<pre class="prompt-block">{selectedLog.response_text}</pre>
							{/if}
						</div>
					</div>
				{:else}
					<!-- Config + call log list -->
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
						<div class="field muted">Reaction req id: {snap.inference.reaction_req_id}</div>
					</div>
					<div class="section">
						<h4>Call Log ({snap.inference.call_log.length})</h4>
						{#if snap.inference.call_log.length > 0}
							{@const avgMs = Math.round(snap.inference.call_log.reduce((s, e) => s + e.duration_ms, 0) / snap.inference.call_log.length)}
							{@const errorCount = snap.inference.call_log.filter(e => e.error).length}
							<div class="field muted">Avg latency: {avgMs}ms | Errors: {errorCount}</div>
							<div class="call-log">
								{#each [...snap.inference.call_log].reverse() as entry}
									<button class="log-entry-btn" class:log-error={entry.error} on:click={() => selectLogEntry(entry.request_id)}>
										<div class="log-header">
											<span class="muted">[{entry.timestamp}]</span>
											<span class="log-id">#{entry.request_id}</span>
											<span class="log-model">{entry.model}</span>
											{#if entry.streaming}<span class="log-badge stream">STREAM</span>{/if}
											{#if entry.error}<span class="log-badge error">ERROR</span>{:else}<span class="log-badge ok">OK</span>{/if}
										</div>
										<div class="log-meta">
											<span>{entry.duration_ms}ms</span>
											<span class="muted">prompt: {entry.prompt_len}ch</span>
											<span class="muted">response: {entry.response_len}ch</span>
										</div>
										{#if entry.error}
											<div class="log-error-msg">{entry.error}</div>
										{/if}
									</button>
								{/each}
							</div>
						{:else}
							<div class="field muted">(no calls yet)</div>
						{/if}
					</div>
				{/if}
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
		overflow-x: auto;
	}

	.gossip-item,
	.conv-entry {
		margin-bottom: 0.3rem;
		padding-bottom: 0.3rem;
		border-bottom: 1px dashed var(--color-border);
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

	.call-log {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		margin-top: 0.25rem;
	}

	.log-entry-btn {
		display: block;
		width: 100%;
		padding: 0.3rem 0.5rem;
		border: none;
		border-bottom: 1px solid var(--color-border);
		background: none;
		cursor: pointer;
		text-align: left;
		font-size: 0.75rem;
		color: var(--color-fg);
	}

	.log-entry-btn:hover {
		background: var(--color-input-bg);
	}

	.log-entry-btn.log-error {
		background: color-mix(in srgb, #ff4444 8%, transparent);
	}

	.log-entry-btn.log-error:hover {
		background: color-mix(in srgb, #ff4444 14%, transparent);
	}

	.prompt-block {
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		padding: 0.4rem 0.5rem;
		font-size: 0.65rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		max-height: 12rem;
		overflow-y: auto;
		color: var(--color-fg);
		margin: 0.15rem 0 0;
	}

	.log-header {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		align-items: baseline;
	}

	.log-id {
		color: var(--color-muted);
		font-size: 0.65rem;
	}

	.log-model {
		color: var(--color-accent);
		font-weight: 600;
	}

	.log-badge {
		font-size: 0.55rem;
		padding: 0.05rem 0.3rem;
		border-radius: 2px;
		text-transform: uppercase;
		font-weight: 700;
		letter-spacing: 0.05em;
	}

	.log-badge.stream {
		background: color-mix(in srgb, var(--color-accent) 20%, transparent);
		color: var(--color-accent);
	}

	.log-badge.ok {
		background: color-mix(in srgb, #44cc44 20%, transparent);
		color: #44cc44;
	}

	.log-badge.error {
		background: color-mix(in srgb, #ff4444 20%, transparent);
		color: #ff4444;
	}

	.log-meta {
		display: flex;
		gap: 0.6rem;
		font-size: 0.65rem;
		margin-top: 0.1rem;
	}

	.log-error-msg {
		color: #ff4444;
		font-size: 0.65rem;
		margin-top: 0.1rem;
		word-break: break-word;
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
</style>
