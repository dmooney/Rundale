<script lang="ts">
	import { worldState } from '../stores/game';
	import { onMount } from 'svelte';

	let displayHour = $state(0);
	let displayMinute = $state(0);
	let displayTimeLabel = $state('');

	// Anchor for client-side clock interpolation
	let anchorRealMs = 0;
	let anchorGameMs = 0;
	let speedFactor = 36.0;
	let paused = false;

	function timeOfDayLabel(hour: number): string {
		if (hour >= 5 && hour < 9) return 'Morning';
		if (hour >= 9 && hour < 12) return 'Late Morning';
		if (hour >= 12 && hour < 14) return 'Midday';
		if (hour >= 14 && hour < 17) return 'Afternoon';
		if (hour >= 17 && hour < 20) return 'Dusk';
		if (hour >= 20 && hour < 22) return 'Evening';
		return 'Night';
	}

	function tick() {
		if (paused) {
			// Use the anchored game time directly
			const d = new Date(anchorGameMs);
			displayHour = d.getUTCHours();
			displayMinute = d.getUTCMinutes();
		} else {
			const elapsedRealMs = Date.now() - anchorRealMs;
			const currentGameMs = anchorGameMs + elapsedRealMs * speedFactor;
			const d = new Date(currentGameMs);
			displayHour = d.getUTCHours();
			displayMinute = d.getUTCMinutes();
		}
		displayTimeLabel = timeOfDayLabel(displayHour);
		requestAnimationFrame(tick);
	}

	// Re-anchor whenever we get a new world snapshot from the backend
	$effect(() => {
		const snap = $worldState;
		if (snap) {
			anchorRealMs = Date.now();
			anchorGameMs = snap.game_epoch_ms;
			speedFactor = snap.speed_factor;
			paused = snap.paused;
		}
	});

	onMount(() => {
		requestAnimationFrame(tick);
	});
</script>

<div class="status-bar">
	{#if $worldState}
		<span class="location">{$worldState.location_name}</span>
		<span class="sep">|</span>
		<span class="time-label">{displayTimeLabel}</span>
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
		<span class="spacer"></span>
		<span class="clock">{#each displayHour.toString().padStart(2, '0').split('') as d}<span class="digit">{d}</span>{/each}<span class="colon">:</span>{#each displayMinute.toString().padStart(2, '0').split('') as d}<span class="digit">{d}</span>{/each}</span>
	{:else}
		<span class="muted">Loading…</span>
	{/if}
</div>

<style>
	.status-bar {
		background: var(--color-panel-bg);
		border-bottom: 1px solid var(--color-border);
		padding: 0.4rem 1rem;
		font-size: 1rem;
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-fg);
		white-space: nowrap;
		overflow: hidden;
	}

	.spacer {
		flex: 1;
	}

	.clock {
		display: inline-flex;
		align-items: baseline;
	}

	.digit {
		display: inline-block;
		width: 0.5em;
		text-align: center;
	}

	.colon {
		display: inline-block;
		width: 0.2em;
		text-align: center;
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
