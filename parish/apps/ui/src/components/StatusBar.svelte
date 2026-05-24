<script lang="ts">
	import { worldState } from '../stores/game';
	import { debugVisible } from '../stores/debug';
	import { savePickerVisible, modSelectorVisible } from '../stores/save';
	import { onDestroy } from 'svelte';
	import AuthStatus from './AuthStatus.svelte';

	let displayHour = $state(0);
	let displayMinute = $state(0);
	let displayTimeLabel = $state('');

	// Anchor for client-side clock interpolation
	let anchorRealMs = 0;
	let anchorGameMs = 0;
	let speedFactor = 36.0;
	let clockFrozen = $state(false);

	let rafId: number;

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
		// When frozen, don't schedule another frame — the display is already
		// set to anchorGameMs by the snapshot $effect below.
		if (clockFrozen) return;
		const elapsedRealMs = Date.now() - anchorRealMs;
		const currentGameMs = anchorGameMs + elapsedRealMs * speedFactor;
		const d = new Date(currentGameMs);
		displayHour = d.getUTCHours();
		displayMinute = d.getUTCMinutes();
		displayTimeLabel = timeOfDayLabel(displayHour);
		rafId = requestAnimationFrame(tick);
	}

	// Re-anchor whenever we get a new world snapshot from the backend.
	// When the clock freezes, update the display once and let the rAF loop
	// stop naturally (tick() bails on the next frame). When it unfreezes,
	// restart the loop.
	//
	// Only `snap.paused` (user-initiated) freezes the visible clock. The
	// transient `inference_paused` flag toggles many times per turn while
	// the LLM runs; folding it into `clockFrozen` made the digits oscillate
	// between running and paused several times per demo turn.
	$effect(() => {
		const snap = $worldState;
		if (snap) {
			anchorRealMs = Date.now();
			anchorGameMs = snap.game_epoch_ms;
			speedFactor = snap.speed_factor;
			clockFrozen = snap.paused;

			if (clockFrozen) {
				// Snap display to anchored game time immediately.
				const d = new Date(anchorGameMs);
				displayHour = d.getUTCHours();
				displayMinute = d.getUTCMinutes();
				displayTimeLabel = timeOfDayLabel(displayHour);
			}
		}
	});

	// React to clockFrozen transitions: cancel the loop when frozen,
	// restart it when unfrozen.
	$effect(() => {
		if (clockFrozen) {
			cancelAnimationFrame(rafId);
		} else {
			// Start (or restart) the rAF loop.
			rafId = requestAnimationFrame(tick);
		}
	});

	onDestroy(() => {
		cancelAnimationFrame(rafId);
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
		<button type="button" class="mod-toggle" aria-label="Switch active mod" onclick={() => modSelectorVisible.set(true)} title="Switch mod">Mod</button>
		<button type="button" class="save-toggle" class:save-active={$savePickerVisible} aria-pressed={$savePickerVisible} aria-label="Save/Load picker" onclick={() => savePickerVisible.update(v => !v)} title="Save/Load picker (F5)">Ledger</button>
		<a class="designer-link" href="/editor" title="Parish Designer — edit mod data">Designer</a>
		<button type="button" class="debug-toggle" class:debug-active={$debugVisible} aria-pressed={$debugVisible} aria-label="Toggle debug panel" onclick={() => debugVisible.update(v => !v)} title="Toggle debug panel (F12)">Dbg</button>
		<AuthStatus />
		<span class="clock">{#each displayHour.toString().padStart(2, '0').split('') as d}<span class="digit">{d}</span>{/each}<span class="colon">:</span>{#each displayMinute.toString().padStart(2, '0').split('') as d}<span class="digit">{d}</span>{/each}</span>
	{:else}
		<span class="muted">Loading…</span>
	{/if}
</div>

<style>
	.status-bar {
		background: var(--status-bg);
		border-bottom: var(--status-border-bottom);
		padding: 0.32rem 1rem;
		font-family: var(--font-display);
		font-size: 0.7rem;
		letter-spacing: 0.07em;
		display: flex;
		align-items: center;
		gap: 0.55rem;
		color: var(--status-fg);
		white-space: nowrap;
		overflow: hidden;
	}

	.spacer {
		flex: 1;
	}

	.clock {
		display: inline-flex;
		align-items: baseline;
		background: var(--status-clock-bg);
		border: 1px solid var(--status-border);
		padding: 0.1rem 0.5rem;
		letter-spacing: 0.1em;
		font-size: 0.78rem;
		color: var(--status-clock-fg);
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
		color: var(--status-sep-fg);
		font-size: 0.7rem;
		letter-spacing: 0;
		opacity: 0.8;
	}

	.location {
		font-family: var(--font-body);
		font-style: italic;
		font-size: 1.05rem;
		font-weight: normal;
		color: var(--status-accent-fg);
		letter-spacing: 0.02em;
	}

	.time-label,
	.weather,
	.season,
	.day-of-week {
		color: var(--status-muted-fg);
	}

	.festival {
		color: var(--status-accent-fg);
	}

	.paused {
		color: var(--status-muted-fg);
		font-style: italic;
	}

	.muted {
		color: var(--status-muted-fg);
		font-style: italic;
	}

	.mod-toggle,
	.save-toggle {
		background: none;
		border: 1px solid var(--status-border);
		color: var(--status-muted-fg);
		font-size: 0.6rem;
		padding: 0.1rem 0.45rem;
		cursor: pointer;
		font-family: var(--font-display);
		letter-spacing: 0.1em;
		transition: color 0.2s, border-color 0.2s;
	}

	.mod-toggle:hover,
	.mod-toggle:focus-visible,
	.save-toggle:hover,
	.save-toggle:focus-visible {
		color: var(--status-fg);
		border-color: var(--status-accent-fg);
	}

	.save-toggle.save-active {
		color: var(--status-accent-fg);
		border-color: var(--status-accent-fg);
	}

	.debug-toggle,
	.designer-link {
		background: none;
		border: 1px solid var(--status-border);
		color: var(--status-muted-fg);
		font-size: 0.6rem;
		padding: 0.1rem 0.45rem;
		cursor: pointer;
		font-family: var(--font-display);
		letter-spacing: 0.1em;
		transition: color 0.2s, border-color 0.2s;
		text-decoration: none;
		display: inline-flex;
		align-items: center;
	}

	.debug-toggle:hover,
	.debug-toggle:focus-visible,
	.designer-link:hover,
	.designer-link:focus-visible {
		color: var(--status-fg);
		border-color: var(--status-accent-fg);
	}

	.debug-toggle.debug-active {
		color: var(--status-accent-fg);
		border-color: var(--status-accent-fg);
	}

	/* ── Mobile: compact status bar ── */
	@media (max-width: 768px) {
		.status-bar {
			padding: 0.3rem 0.6rem;
			gap: 0.35rem;
			font-size: 0.6rem;
		}

		/* Hide non-essential items to prevent overflow */
		.day-of-week,
		.season,
		.weather {
			display: none;
		}

		/* Also hide separators adjacent to hidden items — CSS can't target those
		   individually, so we hide all seps and re-show the one between location
		   and time-label via the adjacent sibling combinator. */
		.sep {
			display: none;
		}

		.location + .sep {
			display: inline;
		}

		.mod-toggle,
		.save-toggle,
		.debug-toggle,
		.designer-link {
			font-size: 0.55rem;
			padding: 0.15rem 0.35rem;
		}

		.clock {
			font-size: 0.7rem;
			padding: 0.08rem 0.35rem;
		}
	}
</style>
