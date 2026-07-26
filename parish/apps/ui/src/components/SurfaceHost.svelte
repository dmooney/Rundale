<script lang="ts">
	import { tick } from 'svelte';
	import FullMapOverlay from './FullMapOverlay.svelte';
	import SavePicker from './SavePicker.svelte';
	import DebugPanel from './DebugPanel.svelte';
	import ModSelectorOverlay from './ModSelectorOverlay.svelte';
	import BugReportModal from './BugReportModal.svelte';
	import ShortcutsOverlay from './ShortcutsOverlay.svelte';
	import { bugReportVisible } from '../stores/bugReport';
	import { debugSnapshot, debugVisible } from '../stores/debug';
	import { fullMapOpen, uiConfig } from '../stores/game';
	import { modSelectorVisible, savePickerVisible } from '../stores/save';
	import {
		activeSurface,
		adoptLegacySurface,
		closeSurface,
		legacySurfaceClosed,
		type Surface,
	} from '../stores/surfaceCoordinator';

	let frameEl: HTMLElement | null = $state(null);

	const childOwnsDialog = $derived(
		$activeSurface === 'save' ||
			$activeSurface === 'mod' ||
			$activeSurface === 'bug' ||
			$activeSurface === 'shortcuts',
	);
	const dismissible = $derived(
		$activeSurface !== 'mod' || !$uiConfig?.base_mod_required,
	);

	// Keep legacy visibility stores synchronized while their components still
	// own their local close/load-complete behavior.
	$effect(() => {
		const active = $activeSurface;
		const legacy: Surface | null = $modSelectorVisible
			? 'mod'
			: $bugReportVisible
				? 'bug'
				: $fullMapOpen
					? 'map'
					: $savePickerVisible
						? 'save'
						: $debugVisible
							? 'debug'
							: null;
		if (legacy && active !== legacy) {
			adoptLegacySurface(legacy);
			return;
		}
		if (active === 'map' && !$fullMapOpen) legacySurfaceClosed('map');
		if (active === 'save' && !$savePickerVisible) legacySurfaceClosed('save');
		if (active === 'debug' && !$debugVisible) legacySurfaceClosed('debug');
		if (active === 'mod' && !$modSelectorVisible) legacySurfaceClosed('mod');
		if (active === 'bug' && !$bugReportVisible) legacySurfaceClosed('bug');
	});

	$effect(() => {
		if (!$activeSurface) return;
		void tick().then(() => {
			const preferred = frameEl?.querySelector<HTMLElement>('[data-autofocus]');
			const first = preferred ?? focusableElements()[0] ?? frameEl;
			first?.focus({ preventScroll: true });
		});
	});

	function focusableElements(): HTMLElement[] {
		if (!frameEl) return [];
		return Array.from(
			frameEl.querySelectorAll<HTMLElement>(
				'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
			),
		).filter((element) => {
			if (element.tabIndex < 0) return false;
			if (element.hasAttribute('inert') || element.closest('[inert]'))
				return false;
			let current: HTMLElement | null = element;
			while (current && current !== frameEl) {
				const style = window.getComputedStyle(current);
				if (
					current.hidden ||
					style.display === 'none' ||
					style.visibility === 'hidden'
				) {
					return false;
				}
				current = current.parentElement;
			}
			return true;
		});
	}

	function handleKeydown(event: KeyboardEvent) {
		const active = $activeSurface;
		if (!active) return;
		if (event.key === 'Tab') {
			const focusable = focusableElements();
			if (focusable.length === 0) {
				event.preventDefault();
				frameEl?.focus();
				return;
			}
			const first = focusable[0];
			const last = focusable[focusable.length - 1];
			const focused = document.activeElement;
			const focusIsOutside = !focused || !frameEl?.contains(focused);
			if (
				event.shiftKey &&
				(focused === first || focused === frameEl || focusIsOutside)
			) {
				event.preventDefault();
				last.focus();
			} else if (
				!event.shiftKey &&
				(focused === last || focused === frameEl || focusIsOutside)
			) {
				event.preventDefault();
				first.focus();
			}
			return;
		}
		if (event.key === 'Escape' && (active === 'map' || active === 'debug')) {
			event.preventDefault();
			event.stopPropagation();
			closeSurface(active);
		}
	}

	function title(surface: Surface): string {
		switch (surface) {
			case 'map':
				return 'Parish map';
			case 'save':
				return 'Save or load';
			case 'debug':
				return 'Debug records';
			case 'mod':
				return 'Choose the parish';
			case 'bug':
				return 'Report a problem';
			case 'shortcuts':
				return 'Keyboard shortcuts';
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if $activeSurface}
	<div
		class="surface-backdrop"
		data-testid="surface-backdrop"
		data-surface={$activeSurface}
	>
		{#if dismissible}
			<button
				type="button"
				class="surface-dismiss"
				data-testid="surface-dismiss"
				aria-label={`Dismiss ${title($activeSurface)}`}
				aria-hidden="true"
				tabindex="-1"
				onclick={() => closeSurface($activeSurface ?? undefined)}
			></button>
		{/if}

		<section
			bind:this={frameEl}
			class="surface-frame"
			class:child-dialog={childOwnsDialog}
			class:map={$activeSurface === 'map'}
			data-testid={`surface-${$activeSurface}`}
			role={childOwnsDialog ? undefined : 'dialog'}
			aria-modal={childOwnsDialog ? undefined : 'true'}
			aria-label={title($activeSurface)}
			tabindex="-1"
		>
			{#if !childOwnsDialog}
				<header class="surface-header">
					<div>
						<span class="eyebrow">Rundale</span>
						<h2>{title($activeSurface)}</h2>
					</div>
					{#if dismissible}
						<button
							type="button"
							class="surface-close"
							aria-label={`Close ${title($activeSurface)}`}
							data-autofocus
							onclick={() => closeSurface($activeSurface ?? undefined)}
							>Close</button
						>
					{/if}
				</header>
			{/if}

			<div class="surface-body">
				{#if $activeSurface === 'map'}
					<FullMapOverlay onclose={() => closeSurface('map')} />
				{:else if $activeSurface === 'save'}
					<SavePicker />
				{:else if $activeSurface === 'debug'}
					{#if $debugSnapshot}
						<DebugPanel />
					{:else}
						<p class="loading-note">Opening debug records…</p>
					{/if}
				{:else if $activeSurface === 'mod'}
					<ModSelectorOverlay
						onclose={() => closeSurface('mod')}
						required={$uiConfig?.base_mod_required}
					/>
				{:else if $activeSurface === 'bug'}
					<BugReportModal />
				{:else if $activeSurface === 'shortcuts'}
					<ShortcutsOverlay onclose={() => closeSurface('shortcuts')} />
				{/if}
			</div>
		</section>
	</div>
{/if}

<style>
	.surface-backdrop {
		position: fixed;
		inset: 0;
		z-index: 80;
		display: grid;
		place-items: center;
		padding: clamp(0.5rem, 3vw, 2rem);
		background: color-mix(in srgb, var(--color-bg) 68%, transparent);
		backdrop-filter: blur(4px);
	}

	.surface-dismiss {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		padding: 0;
		border: 0;
		background: transparent;
	}

	.surface-frame {
		position: relative;
		z-index: 1;
		display: flex;
		flex-direction: column;
		width: min(72rem, 100%);
		height: min(50rem, 100%);
		min-height: 0;
		overflow: hidden;
		color: var(--color-fg);
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 0.5rem;
		box-shadow: 0 1.5rem 5rem rgba(0, 0, 0, 0.45);
	}

	.surface-frame.child-dialog {
		display: block;
		background: transparent;
		border: 0;
		border-radius: 0;
		box-shadow: none;
		pointer-events: none;
	}

	.surface-frame.child-dialog :global(*) {
		pointer-events: auto;
	}

	.surface-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.75rem 1rem;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-bg);
	}

	.eyebrow {
		display: block;
		color: var(--color-muted);
		font: 600 0.62rem/1 var(--font-display);
		letter-spacing: 0.15em;
		text-transform: uppercase;
	}

	h2 {
		margin: 0.2rem 0 0;
		font: 600 1rem/1.2 var(--font-display);
		letter-spacing: 0.04em;
	}

	.surface-close {
		padding: 0.35rem 0.7rem;
		color: var(--color-fg);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 0.25rem;
		cursor: pointer;
	}

	.surface-close:hover,
	.surface-close:focus-visible {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.surface-body {
		position: relative;
		flex: 1;
		min-height: 0;
		overflow: hidden;
	}

	.surface-body :global(.map-embed) {
		position: absolute;
		inset: 0;
	}

	.surface-body :global(.map-embed > .close-btn),
	.surface-body :global(.debug-header) {
		display: none;
	}

	.surface-body :global(.debug-panel),
	.surface-body :global(.debug-panel.left-dock) {
		position: static;
		width: 100%;
		height: 100%;
		border: 0;
	}

	.loading-note {
		padding: 1rem;
		color: var(--color-muted);
	}

	@media (max-width: 768px) {
		.surface-backdrop {
			padding: max(0.4rem, env(safe-area-inset-top))
				max(0.4rem, env(safe-area-inset-right))
				max(0.4rem, env(safe-area-inset-bottom))
				max(0.4rem, env(safe-area-inset-left));
		}

		.surface-frame {
			width: 100%;
			height: 100%;
			border-radius: 0.35rem;
		}
	}
</style>
