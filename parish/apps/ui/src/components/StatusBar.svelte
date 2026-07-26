<script lang="ts">
	import { worldState, externalDriveActive } from '../stores/game';
	import { debugVisible } from '../stores/debug';
	import { openBugReport } from '../stores/bugReport';
	import { savePickerVisible, modSelectorVisible } from '../stores/save';
	import { onDestroy } from 'svelte';
	import { resolve } from '$app/paths';
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

	/** Mirrors the backend `TimeOfDay` buckets (parish-types/src/time.rs) so
	 *  the interpolated label never disagrees with snapshot-driven labels
	 *  (e.g. the chat log's time-rule separators). */
	function timeOfDayLabel(hour: number): string {
		if (hour >= 5 && hour < 7) return 'Dawn';
		if (hour >= 7 && hour < 12) return 'Morning';
		if (hour >= 12 && hour < 14) return 'Midday';
		if (hour >= 14 && hour < 17) return 'Afternoon';
		if (hour >= 17 && hour < 19) return 'Dusk';
		if (hour >= 19 && hour < 23) return 'Night';
		return 'Midnight';
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

	// Dev-tools menu (⋯): Designer / Dbg / bug-report / Mod live behind one
	// button so player chrome (Ledger, clock) isn't visually level with
	// developer chrome.
	let devMenuOpen = $state(false);
	let devMenuEl: HTMLDivElement | undefined = $state();

	function handleWindowPointerDown(e: PointerEvent) {
		if (devMenuOpen && devMenuEl && !devMenuEl.contains(e.target as Node)) {
			devMenuOpen = false;
		}
	}

	function handleDevMenuKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && devMenuOpen) {
			devMenuOpen = false;
		}
	}
</script>

<svelte:window onpointerdown={handleWindowPointerDown} onkeydown={handleDevMenuKeydown} />

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
		{#if $externalDriveActive}
			<span class="auto-play-badge" role="status" aria-live="polite" aria-label="Game is being driven by an automated agent" title="An automated agent is driving the game">&#8635; Auto-play</span>
		{/if}
		<button type="button" class="save-toggle" class:save-active={$savePickerVisible} aria-pressed={$savePickerVisible} aria-label="Save/Load picker" onclick={() => savePickerVisible.update(v => !v)} title="Save/Load picker (F5)">Ledger</button>
		<div
			class="dev-menu-wrap"
			bind:this={devMenuEl}
			onfocusout={(e) => {
				// Close when keyboard focus leaves the toggle + menu subtree so
				// tabbing away doesn't leave the menu hanging open.
				const next = e.relatedTarget as Node | null;
				if (devMenuOpen && devMenuEl && (!next || !devMenuEl.contains(next))) {
					devMenuOpen = false;
				}
			}}
		>
			<button
				type="button"
				class="dev-toggle"
				class:dev-active={devMenuOpen}
				aria-haspopup="menu"
				aria-expanded={devMenuOpen}
				aria-label="Developer tools menu"
				title="Developer tools"
				onclick={() => (devMenuOpen = !devMenuOpen)}
			>⋯</button>
			{#if devMenuOpen}
				<div class="dev-menu" role="menu" aria-label="Developer tools" data-testid="dev-menu">
					<button type="button" role="menuitem" class="dev-item" aria-label="Switch active mod" onclick={() => { devMenuOpen = false; modSelectorVisible.set(true); }} title="Switch mod">Mod</button>
					<a role="menuitem" class="dev-item" href={resolve('/editor')} title="Parish Designer — edit mod data">Designer</a>
					<button type="button" role="menuitemcheckbox" class="dev-item" class:debug-active={$debugVisible} aria-checked={$debugVisible} aria-label="Toggle debug panel" onclick={() => { devMenuOpen = false; debugVisible.update(v => !v); }} title="Toggle debug panel (F12)">Dbg</button>
					<button type="button" role="menuitem" class="dev-item" aria-label="Report a bug" onclick={() => { devMenuOpen = false; void openBugReport(); }} title="Report a bug">🐛 Bug</button>
				</div>
			{/if}
		</div>
		<AuthStatus />
		<span class="clock">{#each displayHour.toString().padStart(2, '0').split('') as d, i (i)}<span class="digit">{d}</span>{/each}<span class="colon">:</span>{#each displayMinute.toString().padStart(2, '0').split('') as d, i (i)}<span class="digit">{d}</span>{/each}</span>
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
		font-size: 0.76rem;
		letter-spacing: 0.07em;
		display: flex;
		align-items: center;
		gap: 0.55rem;
		color: var(--color-muted);
		white-space: nowrap;
		/* Allow the absolutely-positioned dev-menu dropdown to overflow vertically
		 * while still clipping horizontal overflow (location text ellipsis, etc.).
		 * overflow: hidden would clip the dropdown behind the bar (#1431 item 5). */
		overflow-x: clip;
		overflow-y: visible;
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

	/* Auto-play badge: visible when an external agent is driving the game (#1537). */
	.auto-play-badge {
		font-family: var(--font-display);
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		color: var(--color-accent);
		border: 1px solid var(--color-accent);
		padding: 0.1rem 0.45rem;
		opacity: 0.85;
		white-space: nowrap;
		animation: auto-play-pulse 2s ease-in-out infinite;
	}

	@keyframes auto-play-pulse {
		0%   { opacity: 0.85; }
		50%  { opacity: 0.45; }
		100% { opacity: 0.85; }
	}

	.save-toggle,
	.dev-toggle {
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

	.save-toggle:hover,
	.save-toggle:focus-visible,
	.dev-toggle:hover,
	.dev-toggle:focus-visible {
		color: var(--color-fg);
		border-color: var(--color-accent);
	}

	.save-toggle.save-active,
	.dev-toggle.dev-active {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.dev-menu-wrap {
		position: relative;
		display: inline-flex;
	}

	.dev-menu {
		position: absolute;
		top: calc(100% + 0.35rem);
		right: 0;
		z-index: 60;
		display: flex;
		flex-direction: column;
		min-width: 7rem;
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.18);
	}

	.dev-item {
		background: none;
		border: none;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-muted);
		font-size: 0.62rem;
		padding: 0.4rem 0.6rem;
		cursor: pointer;
		font-family: var(--font-display);
		letter-spacing: 0.1em;
		text-decoration: none;
		text-align: left;
		transition: color 0.2s, background 0.2s;
	}

	.dev-item:last-child {
		border-bottom: none;
	}

	.dev-item:hover,
	.dev-item:focus-visible {
		color: var(--color-fg);
		background: var(--color-input-bg);
	}

	.dev-item.debug-active {
		color: var(--color-accent);
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

		.save-toggle,
		.dev-toggle {
			font-size: 0.55rem;
			padding: 0.15rem 0.35rem;
		}

		.clock {
			font-size: 0.7rem;
			padding: 0.08rem 0.35rem;
		}
	}
</style>
