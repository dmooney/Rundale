<script lang="ts">
	import { worldState } from '../stores/game';
</script>

<div class="status-bar">
	{#if $worldState}
		<span class="location">{$worldState.location_name}</span>
		<span class="sep">|</span>
		<span class="time">{$worldState.time_label} {$worldState.hour.toString().padStart(2, '0')}:00</span>
		<span class="sep">|</span>
		<span class="weather">{$worldState.weather}</span>
		<span class="sep">|</span>
		<span class="season">{$worldState.season}</span>
		{#if $worldState.festival}
			<span class="sep">|</span>
			<span class="festival">🎉 {$worldState.festival}</span>
		{/if}
		{#if $worldState.paused}
			<span class="sep">|</span>
			<span class="paused">⏸ Paused</span>
		{/if}
	{:else}
		<span class="muted">Loading…</span>
	{/if}
</div>

<style>
	.status-bar {
		background: var(--color-panel-bg);
		border-bottom: 1px solid var(--color-border);
		padding: 0.4rem 1rem;
		font-size: 0.85rem;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-fg);
		white-space: nowrap;
		overflow: hidden;
	}

	.sep {
		color: var(--color-border);
	}

	.location {
		color: var(--color-accent);
		font-weight: 600;
	}

	.festival {
		color: var(--color-accent);
	}

	.paused {
		color: var(--color-muted);
		font-style: italic;
	}

	.muted {
		color: var(--color-muted);
		font-style: italic;
	}
</style>
