<script lang="ts">
	import type { DebugSnapshot } from '$lib/types';

	let { snap }: { snap: DebugSnapshot } = $props();
</script>

<div class="section">
	<h4>Weather Engine</h4>
	<div class="field">Current: <span class="accent">{snap.weather.current}</span></div>
	<div class="field">Since: {snap.weather.since}</div>
	<div class="field">Duration: {snap.weather.duration_hours.toFixed(2)}h</div>
	<div class="field muted">Min duration before next transition: {snap.weather.min_duration_hours}h</div>
	<div class="field muted">Last checked: {snap.weather.last_check_at ?? '(never)'}</div>
</div>

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.accent { color: var(--color-accent); }
	.muted { color: var(--color-muted); }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }
</style>
