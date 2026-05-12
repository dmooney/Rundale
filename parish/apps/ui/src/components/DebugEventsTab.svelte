<script lang="ts">
	import type { DebugSnapshot } from '$lib/types';

	let { snap }: { snap: DebugSnapshot } = $props();
</script>

<div class="section">
	<h4>Game Events ({snap.event_bus.recent_events.length}) \u2014 subscribers: {snap.event_bus.subscriber_count}</h4>
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

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.muted { color: var(--color-muted); }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }

	.event-cat {
		color: var(--color-accent);
		font-size: 0.65rem;
	}
</style>
