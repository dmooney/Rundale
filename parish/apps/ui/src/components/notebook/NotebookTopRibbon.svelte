<script lang="ts">
	import { resolve } from '$app/paths';
	import AuthStatus from '../AuthStatus.svelte';
	import { worldState, externalDriveActive } from '../../stores/game';
	import { debugVisible } from '../../stores/debug';
	import { savePickerVisible, modSelectorVisible } from '../../stores/save';
	import { openBugReport } from '../../stores/bugReport';

	let devMenuOpen = $state(false);
	let devMenuEl: HTMLDivElement | undefined = $state();

	function formattedClock(): string {
		if (!$worldState) return '--:--';
		return `${String($worldState.hour).padStart(2, '0')}:${String($worldState.minute).padStart(2, '0')}`;
	}

	function handleWindowPointerDown(e: PointerEvent) {
		if (devMenuOpen && devMenuEl && !devMenuEl.contains(e.target as Node)) {
			devMenuOpen = false;
		}
	}

	function handleWindowKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' && devMenuOpen) {
			devMenuOpen = false;
		}
	}
</script>

<svelte:window onpointerdown={handleWindowPointerDown} onkeydown={handleWindowKeydown} />

<header class="top-ribbon" data-testid="notebook-top-ribbon">
	<div class="brand-slip">
		<span class="brand">Rundale</span>
		<span class="subtitle">Parish Notebook</span>
	</div>

	{#if $worldState}
		<div class="ribbon-center" aria-label="Current place and conditions">
			<span class="place">{$worldState.location_name}</span>
			<span class="slash"></span>
			<span>{$worldState.time_label}</span>
			<span class="weather">{$worldState.weather}</span>
			<span>{formattedClock()}</span>
			<span class="season">{$worldState.day_of_week} · {$worldState.season}</span>
			{#if $worldState.festival}
				<span class="festival">{$worldState.festival}</span>
			{/if}
			{#if $worldState.paused}
				<span class="paused">Paused</span>
			{/if}
		</div>
	{:else}
		<div class="ribbon-center muted">Opening the notebook…</div>
	{/if}

	<div class="ribbon-actions">
		{#if $externalDriveActive}
			<span class="auto-drive" role="status">Auto-play</span>
		{/if}
		<button
			type="button"
			class:active={$savePickerVisible}
			aria-pressed={$savePickerVisible}
			onclick={() => savePickerVisible.update((v) => !v)}
		>
			Ledger
		</button>
		<div
			class="tools"
			bind:this={devMenuEl}
			onfocusout={(e) => {
				const next = e.relatedTarget as Node | null;
				if (devMenuOpen && devMenuEl && (!next || !devMenuEl.contains(next))) {
					devMenuOpen = false;
				}
			}}
		>
			<button
				type="button"
				class:active={devMenuOpen}
				aria-haspopup="menu"
				aria-expanded={devMenuOpen}
				onclick={() => (devMenuOpen = !devMenuOpen)}
			>
				Tools
			</button>
			{#if devMenuOpen}
				<div class="tools-menu" role="menu" aria-label="Notebook tools">
					<button type="button" role="menuitem" onclick={() => { devMenuOpen = false; modSelectorVisible.set(true); }}>Mod</button>
					<a role="menuitem" href={resolve('/editor')}>Designer</a>
					<button type="button" role="menuitemcheckbox" aria-checked={$debugVisible} class:active={$debugVisible} onclick={() => { devMenuOpen = false; debugVisible.update((v) => !v); }}>Debug</button>
					<button type="button" role="menuitem" onclick={() => { devMenuOpen = false; void openBugReport(); }}>Bug</button>
				</div>
			{/if}
		</div>
		<AuthStatus />
	</div>
</header>

<style>
	.top-ribbon {
		position: absolute;
		left: 0;
		right: 0;
		top: 0;
		z-index: 10;
		display: grid;
		grid-template-columns: minmax(18rem, 23rem) minmax(0, 1fr) minmax(14rem, 22rem);
		align-items: stretch;
		height: clamp(4.35rem, 7.6vh, 5.25rem);
		background: url('/notebook-ui/assets/paper-strip.svg') center / 100% 100%;
		filter: drop-shadow(0 6px 10px rgba(22, 17, 10, 0.28));
		color: var(--notebook-ink);
	}

	.top-ribbon::after {
		content: '';
		position: absolute;
		left: 0.4rem;
		right: 0.4rem;
		bottom: 0.52rem;
		border-top: 1px solid rgba(45, 32, 18, 0.18);
		pointer-events: none;
	}

	.brand-slip {
		display: grid;
		align-content: center;
		gap: 0.05rem;
		padding: 0.55rem 1.6rem 0.8rem 2rem;
		border-right: 1px solid rgba(45, 32, 18, 0.34);
	}

	.brand {
		font-family: var(--font-display);
		font-size: clamp(1.35rem, 2.5vw, 2.1rem);
		line-height: 1;
		letter-spacing: 0.2em;
		text-transform: uppercase;
	}

	.subtitle {
		font-family: var(--font-body);
		font-style: italic;
		font-size: 0.86rem;
		color: var(--notebook-ink-soft);
	}

	.ribbon-center {
		min-width: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: clamp(0.6rem, 1.6vw, 1.35rem);
		padding: 0.45rem 1rem 0.8rem;
		font-family: var(--font-body);
		font-style: italic;
		font-size: clamp(1rem, 1.55vw, 1.34rem);
		white-space: nowrap;
		overflow: hidden;
	}

	.ribbon-center > span {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.place {
		font-size: 1.1em;
		color: var(--notebook-ink);
	}

	.slash {
		width: 5rem;
		border-top: 1px solid rgba(45, 32, 18, 0.28);
		transform: rotate(-2deg);
	}

	.weather,
	.season,
	.festival,
	.paused,
	.muted {
		color: var(--notebook-ink-soft);
	}

	.paused,
	.auto-drive {
		border: 1px solid color-mix(in srgb, var(--color-accent) 40%, transparent);
		border-radius: 999px;
		padding: 0.1rem 0.45rem;
		background: rgba(255, 251, 229, 0.55);
	}

	.ribbon-actions {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 0.45rem;
		min-width: 0;
		padding: 0.55rem 1.15rem 0.9rem 0.75rem;
	}

	button,
	a {
		border: 1px solid rgba(75, 53, 25, 0.24);
		border-radius: 0.35rem;
		background: rgba(238, 220, 174, 0.55);
		color: var(--notebook-ink-soft);
		font-family: var(--font-display);
		font-size: 0.64rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		text-decoration: none;
		padding: 0.35rem 0.55rem;
		cursor: pointer;
	}

	button:hover,
	button:focus-visible,
	button.active,
	a:hover,
	a:focus-visible {
		color: var(--notebook-ink);
		border-color: color-mix(in srgb, var(--color-accent) 55%, var(--notebook-ink));
		background: rgba(255, 248, 220, 0.75);
	}

	.tools {
		position: relative;
	}

	.tools-menu {
		position: absolute;
		right: 0;
		top: calc(100% + 0.35rem);
		display: grid;
		gap: 0.25rem;
		min-width: 8rem;
		padding: 0.4rem;
		border: 1px solid rgba(75, 53, 25, 0.24);
		border-radius: 0.45rem;
		background: #dec894;
		box-shadow: 0 10px 24px rgba(39, 29, 16, 0.22);
	}

	.tools-menu button,
	.tools-menu a {
		width: 100%;
		text-align: left;
	}

	.auto-drive {
		color: var(--notebook-ink-soft);
		font-family: var(--font-display);
		font-size: 0.62rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}

	@media (max-width: 900px) {
		.top-ribbon {
			position: relative;
			grid-template-columns: 1fr;
			height: auto;
			min-height: 6.75rem;
			overflow: visible;
		}

		.brand-slip,
		.ribbon-center,
		.ribbon-actions {
			border-right: 0;
			justify-content: center;
		}

		.brand-slip {
			justify-items: center;
			padding: 0.55rem 0.75rem 0.3rem;
			text-align: center;
		}

		.brand {
			max-width: 100%;
			font-size: 1.32rem;
			letter-spacing: 0.13em;
		}

		.subtitle {
			font-size: 0.78rem;
		}

		.ribbon-center {
			flex-wrap: wrap;
			white-space: normal;
			justify-content: center;
			gap: 0.45rem 0.7rem;
			min-height: 2rem;
			padding: 0.25rem 0.75rem 0.45rem;
			font-size: 0.88rem;
			border-top: 1px solid rgba(75, 53, 25, 0.16);
		}

		.slash {
			display: none;
		}

		.ribbon-actions {
			justify-content: center;
			flex-wrap: wrap;
			padding: 0.35rem 0.75rem 0.55rem;
			border-top: 1px solid rgba(75, 53, 25, 0.12);
		}
	}
</style>
