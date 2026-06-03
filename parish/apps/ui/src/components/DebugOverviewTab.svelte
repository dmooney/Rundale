<script lang="ts">
	import type { DebugSnapshot } from '$lib/types';

	let { snap }: { snap: DebugSnapshot } = $props();
</script>

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
	<div class="field">Player name: {#if snap.world.player_name}<span class="accent">{snap.world.player_name}</span>{:else}<span class="muted">(unknown)</span>{/if}</div>
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
	<div class="field">T2 background:
		{#if snap.tier_summary.tier2_in_flight}
			<span class="accent">IN FLIGHT</span>
		{:else}
			idle
		{/if}
		{#if snap.tier_summary.last_tier2_tick}
			| last: {snap.tier_summary.last_tier2_tick}
		{:else}
			| (never run)
		{/if}
	</div>
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
		<span class="muted">| pending: {snap.tier_summary.tier3_pending_count}</span>
	</div>
	<div class="field muted">
		T2 last: {snap.tier_summary.last_tier2_tick ?? '(never)'}
		| T4 last: {snap.tier_summary.last_tier4_tick ?? '(never)'}
	</div>
	{#if snap.tier_summary.tier4_recent_events.length > 0}
		<div class="field">Recent life events:</div>
		{#each snap.tier_summary.tier4_recent_events.slice(-3) as evt, i (i)}
			<div class="field muted">- {evt}</div>
		{/each}
	{/if}
</div>
<div class="section">
	<h4>Event Bus</h4>
	<div class="field muted">
		Subscribers: {snap.event_bus.subscriber_count}
		| Captured: {snap.event_bus.recent_events.length}
	</div>
</div>
<div class="section">
	<h4>Gossip ({snap.gossip.item_count})</h4>
	{#if snap.gossip.items.length > 0}
		{#each snap.gossip.items.slice(-3) as item (item.id)}
			<div class="field muted">- {item.content.length > 80 ? item.content.slice(0, 77) + '...' : item.content}</div>
		{/each}
	{/if}
</div>
<div class="section">
	<h4>Auth</h4>
	{#if snap.auth.oauth_enabled}
		{#if snap.auth.logged_in}
			<div class="field">
				<span class="accent">Signed in</span>
				{#if snap.auth.provider}<span class="muted"> via {snap.auth.provider}</span>{/if}
			</div>
			{#if snap.auth.display_name}
				<div class="field muted">User: {snap.auth.display_name}</div>
			{/if}
		{:else}
			<div class="field muted">OAuth enabled  not signed in</div>
		{/if}
	{:else}
		<div class="field muted">OAuth disabled (no credentials configured)</div>
	{/if}
	{#if snap.auth.session_id}
		<div class="field muted">Session: {snap.auth.session_id}</div>
	{/if}
</div>

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.accent { color: var(--color-accent); }
	.muted { color: var(--color-muted); }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }
</style>
