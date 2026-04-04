<script lang="ts">
	import { worldState } from '../stores/game';
	import { debugVisible } from '../stores/debug';
	import { savePickerVisible } from '../stores/save';
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

<div class="status-bar" data-testid="status-bar">
	{#if $worldState}
		<span class="location">{$worldState.location_name}</span>
		<span class="sep">·</span>
		<span class="time-label">{displayTimeLabel}</span>
		<span class="sep">·</span>
		<span class="day-of-week">{$worldState.day_of_week}</span>
		<span class="sep">·</span>
		<span class="weather">{$worldState.weather}</span>
		<span class="sep">·</span>
		<span class="season">{$worldState.season}</span>
		{#if $worldState.festival}
			<span class="sep">·</span>
			<span class="festival">✦ {$worldState.festival}</span>
		{/if}
		{#if $worldState.paused}
			<span class="sep">·</span>
			<span class="paused">⏸ Paused</span>
		{/if}
		<span class="spacer"></span>
		<button class="save-toggle" class:save-active={$savePickerVisible} onclick={() => savePickerVisible.update(v => !v)} title="Save/Load picker (F5)">Ledger</button>
		<button class="debug-toggle" class:debug-active={$debugVisible} onclick={() => debugVisible.update(v => !v)} title="Toggle debug panel (F12)">Dbg</button>
		<span class="clock">{#each displayHour.toString().padStart(2, '0').split('') as d}<span class="digit">{d}</span>{/each}<span class="colon">:</span>{#each displayMinute.toString().padStart(2, '0').split('') as d}<span class="digit">{d}</span>{/each}</span>
	{:else}
		<span class="muted">Loading…</span>
	{/if}
</div>

<style>
	.status-bar {
		background: var(--color-panel-bg);
		border-bottom: 1px solid var(--color-border);
		padding: 0.32rem 1rem;
		font-family: var(--font-display);
		font-size: 0.7rem;
		letter-spacing: 0.07em;
		display: flex;
		align-items: center;
		gap: 0.55rem;
		color: var(--color-muted);
		white-space: nowrap;
		overflow: hidden;
	}

	.spacer {
		flex: 1;
	}

	.clock {
		display: inline-flex;
		align-items: baseline;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		padding: 0.1rem 0.5rem;
		letter-spacing: 0.1em;
		font-size: 0.78rem;
		color: var(--color-fg);
	}

	.digit {
		display: inline-block;
		width: 0.55em;
		text-align: center;
	}

	.colon {
		display: inline-block;
		width: 0.2em;
		text-align: center;
	}

	.sep {
		color: var(--color-border);
		font-size: 0.7rem;
		letter-spacing: 0;
		opacity: 0.8;
	}

	.location {
		font-family: var(--font-body);
		font-style: italic;
		font-size: 1.05rem;
		font-weight: normal;
		color: var(--color-accent);
		letter-spacing: 0.02em;
	}

	.time-label,
	.weather,
	.season,
	.day-of-week {
		color: var(--color-muted);
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

	.save-toggle {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		font-size: 0.6rem;
		padding: 0.1rem 0.45rem;
		cursor: pointer;
		font-family: var(--font-display);
		letter-spacing: 0.1em;
		transition: color 0.2s, border-color 0.2s;
	}

	.save-toggle:hover {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	.save-toggle.save-active {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.debug-toggle {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		font-size: 0.6rem;
		padding: 0.1rem 0.45rem;
		cursor: pointer;
		font-family: var(--font-display);
		letter-spacing: 0.1em;
		transition: color 0.2s, border-color 0.2s;
	}

	.debug-toggle:hover {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	.debug-toggle.debug-active {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}
</style>
