<script lang="ts">
	import { tick } from 'svelte';
	import type { NotebookSurface } from '$lib/illustrated-parish/types';
	import ChatPanel from '../ChatPanel.svelte';
	import Sidebar from '../Sidebar.svelte';
	import FullMapOverlay from '../FullMapOverlay.svelte';
	import SavePicker from '../SavePicker.svelte';
	import DebugPanel from '../DebugPanel.svelte';
	import ModSelectorOverlay from '../ModSelectorOverlay.svelte';
	import BugReportModal from '../BugReportModal.svelte';
	import ShortcutsOverlay from '../ShortcutsOverlay.svelte';
	import { bugReportVisible } from '../../stores/bugReport';
	import { debugSnapshot, debugVisible } from '../../stores/debug';
	import {
		fullMapOpen,
		npcsHere,
		playerSubmittedCount,
		streamingActive,
		uiConfig,
		worldState,
	} from '../../stores/game';
	import { modSelectorVisible, savePickerVisible } from '../../stores/save';
	import {
		adoptLegacyNotebookSurface,
		closeNotebookOverlay,
		legacyNotebookSurfaceClosed,
		notebookOverlay,
		notebookPersonSelection,
		openNotebookOverlay,
	} from '../../stores/notebookOverlay';

	let frameEl: HTMLElement | null = $state(null);

	const activeSurface = $derived($notebookOverlay);
	const childOwnsDialog = $derived(
		activeSurface === 'save' ||
			activeSurface === 'mod' ||
			activeSurface === 'bug' ||
			activeSurface === 'shortcuts',
	);
	const dismissible = $derived(
		activeSurface !== 'mod' || !$uiConfig?.base_mod_required,
	);
	const frameKind = $derived(
		activeSurface === 'utility'
			? 'utility'
			: activeSurface === 'journal' ||
				  activeSurface === 'people' ||
				  activeSurface === 'focail' ||
				  activeSurface === 'rumours' ||
				  activeSurface === 'time' ||
				  activeSurface === 'intents'
				? 'drawer'
				: 'modal',
	);

	$effect(() => {
		const active = $notebookOverlay;
		const legacy: NotebookSurface | null = $modSelectorVisible
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
			adoptLegacyNotebookSurface(legacy);
			return;
		}
		if (active === 'map' && !$fullMapOpen) legacyNotebookSurfaceClosed('map');
		if (active === 'save' && !$savePickerVisible)
			legacyNotebookSurfaceClosed('save');
		if (active === 'debug' && !$debugVisible)
			legacyNotebookSurfaceClosed('debug');
		if (active === 'mod' && !$modSelectorVisible)
			legacyNotebookSurfaceClosed('mod');
		if (active === 'bug' && !$bugReportVisible)
			legacyNotebookSurfaceClosed('bug');
	});

	$effect(() => {
		if (!$notebookOverlay) return;
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
		const active = $notebookOverlay;
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
			const active = document.activeElement;
			const focusIsOutside = !active || !frameEl?.contains(active);
			if (
				event.shiftKey &&
				(active === first || active === frameEl || focusIsOutside)
			) {
				event.preventDefault();
				last.focus();
			} else if (
				!event.shiftKey &&
				(active === last || active === frameEl || focusIsOutside)
			) {
				event.preventDefault();
				first.focus();
			}
			return;
		}
		if (
			event.key === 'Escape' &&
			(active === 'journal' ||
				active === 'people' ||
				active === 'focail' ||
				active === 'utility' ||
				active === 'time' ||
				active === 'intents' ||
				active === 'rumours' ||
				active === 'debug')
		) {
			event.preventDefault();
			event.stopPropagation();
			closeNotebookOverlay(active);
		}
	}

	function closeActive() {
		if ($notebookOverlay) closeNotebookOverlay($notebookOverlay);
	}

	function selectPerson(realName: string) {
		notebookPersonSelection.set(realName);
		closeActive();
	}

	function openFromUtility(surface: NotebookSurface, event: MouseEvent) {
		void openNotebookOverlay(surface, event.currentTarget as HTMLElement);
	}

	function surfaceTitle(surface: NotebookSurface): string {
		switch (surface) {
			case 'journal':
				return 'Parish Journal';
			case 'people':
				return 'People of the Parish';
			case 'focail':
				return 'Focail — Irish Words';
			case 'map':
				return 'Parish Map';
			case 'save':
				return 'The Parish Ledger';
			case 'debug':
				return 'Parish Records';
			case 'mod':
				return 'Choose the Parish';
			case 'bug':
				return 'Report a Problem';
			case 'shortcuts':
				return 'Notebook Shortcuts';
			case 'utility':
				return 'More from the Notebook';
			case 'time':
				return 'Time & Weather';
			case 'intents':
				return 'Active Intents';
			case 'rumours':
				return 'Rumours';
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if activeSurface}
	<div
		class="notebook-overlay-backdrop"
		data-testid="notebook-overlay-backdrop"
	>
		{#if dismissible}
			<button
				type="button"
				class="notebook-overlay-dismiss"
				data-testid="notebook-overlay-dismiss"
				aria-label={`Dismiss ${surfaceTitle(activeSurface)}`}
				aria-hidden="true"
				tabindex="-1"
				onclick={closeActive}
			></button>
		{/if}
		<section
			bind:this={frameEl}
			class="notebook-overlay-frame"
			class:drawer={frameKind === 'drawer'}
			class:modal={frameKind === 'modal'}
			class:utility={frameKind === 'utility'}
			class:legacy-shell={activeSurface === 'map' ||
				activeSurface === 'save' ||
				activeSurface === 'debug' ||
				activeSurface === 'mod' ||
				activeSurface === 'bug' ||
				activeSurface === 'shortcuts' ||
				activeSurface === 'journal' ||
				activeSurface === 'focail'}
			data-testid={`notebook-overlay-${activeSurface}`}
			data-surface={activeSurface}
			role={childOwnsDialog ? undefined : 'dialog'}
			aria-modal={childOwnsDialog ? undefined : 'true'}
			aria-label={surfaceTitle(activeSurface)}
			tabindex="-1"
		>
			{#if !childOwnsDialog}
				<header class="notebook-overlay-header">
					<div>
						<span class="eyebrow">Rundale · Parish Notebook</span>
						<h2>{surfaceTitle(activeSurface)}</h2>
					</div>
					{#if dismissible}
						<button
							type="button"
							class="notebook-close"
							aria-label={`Close ${surfaceTitle(activeSurface)}`}
							data-autofocus
							onclick={closeActive}>Close</button
						>
					{/if}
				</header>
			{/if}

			<div
				class="notebook-overlay-body"
				class:map-surface={activeSurface === 'map'}
			>
				{#if activeSurface === 'journal'}
					<ChatPanel />
				{:else if activeSurface === 'people'}
					<ul class="people-list">
						{#each $npcsHere as npc (npc.real_name)}
							<li>
								<button
									type="button"
									onclick={() => selectPerson(npc.real_name)}
								>
									<span class="person-name">{npc.name}</span>
									<span
										>{npc.occupation || 'parish resident'} · {npc.mood ||
											'watchful'}</span
									>
								</button>
							</li>
						{:else}
							<li class="empty-note">No one is nearby.</li>
						{/each}
					</ul>
				{:else if activeSurface === 'focail'}
					<Sidebar onclose={closeActive} />
				{:else if activeSurface === 'map'}
					<FullMapOverlay onclose={() => closeNotebookOverlay('map')} />
				{:else if activeSurface === 'save'}
					<SavePicker />
				{:else if activeSurface === 'debug'}
					{#if $debugSnapshot}
						<DebugPanel />
					{:else}
						<p class="loading-note">Opening the parish records…</p>
					{/if}
				{:else if activeSurface === 'mod'}
					<ModSelectorOverlay
						onclose={() => closeNotebookOverlay('mod')}
						required={$uiConfig?.base_mod_required}
					/>
				{:else if activeSurface === 'bug'}
					<BugReportModal />
				{:else if activeSurface === 'shortcuts'}
					<ShortcutsOverlay onclose={() => closeNotebookOverlay('shortcuts')} />
				{:else if activeSurface === 'utility'}
					<div class="utility-grid">
						<button
							type="button"
							onclick={(event) => openFromUtility('focail', event)}
						>
							<strong>Focail</strong><span>Irish words gathered here</span>
						</button>
						<button
							type="button"
							onclick={(event) => openFromUtility('save', event)}
						>
							<strong>Save / Load</strong><span>Open the parish ledger</span>
						</button>
						<button
							type="button"
							onclick={(event) => openFromUtility('debug', event)}
						>
							<strong>Debug</strong><span>Inspect parish records</span>
						</button>
						<button
							type="button"
							onclick={(event) => openFromUtility('mod', event)}
						>
							<strong>Mod</strong><span>Choose another parish</span>
						</button>
						<button
							type="button"
							onclick={(event) => openFromUtility('bug', event)}
						>
							<strong>Bug Report</strong><span
								>Record a problem with evidence</span
							>
						</button>
						<button
							type="button"
							onclick={(event) => openFromUtility('shortcuts', event)}
						>
							<strong>Shortcuts</strong><span>See notebook keys</span>
						</button>
					</div>
				{:else if activeSurface === 'time'}
					<div class="ink-notes">
						<p>
							<strong>Clock</strong><span
								>{String($worldState?.hour ?? 0).padStart(2, '0')}:{String(
									$worldState?.minute ?? 0,
								).padStart(2, '0')}</span
							>
						</p>
						<p>
							<strong>Weather</strong><span
								>{$worldState?.weather ?? 'unknown'}</span
							>
						</p>
						<p>
							<strong>Season</strong><span
								>{$worldState?.season ?? 'unknown'}</span
							>
						</p>
						<p>
							<strong>Clock state</strong><span
								>{$worldState?.paused ? 'paused' : 'running'}</span
							>
						</p>
						<p>
							<strong>Parish replies</strong><span
								>{$worldState?.inference_paused ? 'paused' : 'ready'}</span
							>
						</p>
						{#if $worldState?.festival}
							<p>
								<strong>Festival</strong><span>{$worldState.festival}</span>
							</p>
						{/if}
					</div>
				{:else if activeSurface === 'intents'}
					<div class="ink-notes">
						<p>
							<strong>Parish reply</strong><span
								>{$streamingActive ? 'pending' : 'idle'}</span
							>
						</p>
						<p>
							<strong>Lines sent</strong><span>{$playerSubmittedCount}</span>
						</p>
					</div>
				{:else if activeSurface === 'rumours'}
					<div class="rumour-note">
						<p>No rumour is pinned to this page yet.</p>
						<p>
							Listen at the crossroads; ink appears when somebody trusts you
							with a story.
						</p>
					</div>
				{/if}
			</div>
		</section>
	</div>
{/if}

<style>
	.notebook-overlay-backdrop {
		position: fixed;
		inset: 0;
		z-index: 80;
		background:
			radial-gradient(
				circle at 43% 43%,
				rgba(29, 25, 20, 0.08),
				rgba(20, 17, 13, 0.58)
			),
			rgba(26, 23, 18, 0.18);
	}

	.notebook-overlay-dismiss {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: default;
	}

	.notebook-overlay-frame {
		--color-bg: #d5bf96;
		--color-fg: #38352d;
		--color-accent: #795640;
		--color-panel-bg: #e2cfaa;
		--color-input-bg: #eadbbd;
		--color-border: #927f60;
		--color-muted: #675e4d;
		position: absolute;
		z-index: 1;
		display: flex;
		flex-direction: column;
		color: #38352d;
		background:
			linear-gradient(rgba(231, 215, 180, 0.93), rgba(213, 188, 143, 0.95)),
			repeating-linear-gradient(
				4deg,
				rgba(93, 73, 43, 0.035) 0 1px,
				transparent 1px 5px
			);
		border: 1px solid rgba(54, 48, 38, 0.78);
		box-shadow:
			0 22px 60px rgba(21, 17, 12, 0.5),
			inset 0 0 38px rgba(101, 76, 44, 0.13);
		clip-path: polygon(
			0.7% 0,
			99.4% 0.5%,
			100% 98.8%,
			98.7% 100%,
			0.5% 99.2%,
			0 1.2%
		);
		font-family: 'Kalam', 'Bradley Hand', 'Segoe Print', cursive;
		overflow: hidden;
	}

	.notebook-overlay-frame::before {
		content: '';
		position: absolute;
		z-index: 3;
		left: 5px;
		top: 13px;
		bottom: 13px;
		width: 9px;
		pointer-events: none;
		background: repeating-linear-gradient(
			to bottom,
			transparent 0 15px,
			rgba(88, 58, 35, 0.82) 15px 17px,
			transparent 17px 31px
		);
		filter: blur(0.15px);
	}

	.notebook-overlay-frame.drawer {
		top: 7.5vh;
		right: 2vw;
		width: min(31rem, 42vw);
		height: min(82vh, 46rem);
	}

	.notebook-overlay-frame.modal {
		inset: 6vh 6vw;
	}

	.notebook-overlay-frame.utility {
		left: 7vw;
		top: 21vh;
		width: min(34rem, 50vw);
		max-height: 70vh;
	}

	.notebook-overlay-header {
		position: relative;
		z-index: 4;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		flex: 0 0 auto;
		padding: 0.8rem 1.1rem 0.62rem 1.55rem;
		border-bottom: 1px solid rgba(54, 48, 38, 0.45);
		background: rgba(239, 224, 192, 0.28);
	}

	.eyebrow {
		display: block;
		font-size: 0.68rem;
		letter-spacing: 0.08em;
		color: #6b5b45;
	}

	h2 {
		margin: 0.05rem 0 0;
		font:
			400 clamp(1.12rem, 2.2vw, 1.72rem) / 1.1 'Kalam',
			'Bradley Hand',
			cursive;
		color: #37342c;
	}

	.notebook-close {
		border: 0;
		border-bottom: 1px solid rgba(55, 50, 40, 0.62);
		padding: 0.2rem 0.35rem;
		color: #514b3e;
		background: transparent;
		font:
			400 0.92rem 'Kalam',
			'Bradley Hand',
			cursive;
		cursor: pointer;
	}

	.notebook-close:hover,
	.notebook-close:focus-visible {
		color: #8b4939;
		border-color: #8b4939;
	}

	.notebook-overlay-body {
		position: relative;
		flex: 1 1 auto;
		min-height: 0;
		overflow: auto;
		margin: 0.35rem 0.55rem 0.65rem 1rem;
		background: rgba(244, 231, 200, 0.18);
	}

	.notebook-overlay-body.map-surface {
		overflow: hidden;
	}

	.people-list {
		list-style: none;
		margin: 0;
		padding: 0.8rem;
		display: grid;
		gap: 0.55rem;
	}

	.people-list button,
	.utility-grid button {
		width: 100%;
		border: 0;
		border-bottom: 1px solid rgba(65, 57, 44, 0.36);
		padding: 0.65rem 0.7rem;
		text-align: left;
		color: #3c382f;
		background: rgba(238, 222, 188, 0.38);
		font: inherit;
		cursor: pointer;
	}

	.people-list button:hover,
	.people-list button:focus-visible,
	.utility-grid button:hover,
	.utility-grid button:focus-visible {
		background: rgba(246, 234, 209, 0.72);
		color: #7d4638;
	}

	.person-name,
	.utility-grid strong {
		display: block;
		font-size: 1.05rem;
		font-weight: 400;
	}

	.people-list button span:last-child,
	.utility-grid span {
		display: block;
		font-size: 0.78rem;
		color: #6c6250;
	}

	.utility-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0.6rem;
		padding: 0.9rem;
	}

	.ink-notes,
	.rumour-note,
	.loading-note,
	.empty-note {
		padding: 1.1rem 1.25rem;
		font-size: 1rem;
	}

	.ink-notes p {
		display: flex;
		justify-content: space-between;
		gap: 2rem;
		margin: 0;
		padding: 0.65rem 0;
		border-bottom: 1px solid rgba(63, 56, 43, 0.34);
	}

	.rumour-note p {
		margin: 0 0 1rem;
	}

	/* Legacy internals are kept behaviorally intact but contained by the
	   notebook sheet, so their dashboard backdrops and docking cannot alter
	   the Pixi viewport. */
	.legacy-shell :global(.overlay),
	.legacy-shell :global(.overlay-backdrop),
	.legacy-shell :global(.shortcuts-container) {
		position: absolute !important;
		inset: 0 !important;
		z-index: 1 !important;
		width: 100% !important;
		height: 100% !important;
		padding: 0 !important;
		background: transparent !important;
	}

	.legacy-shell :global(.modal),
	.legacy-shell :global(.overlay-panel),
	.legacy-shell :global(.shortcuts-card) {
		width: 100% !important;
		max-width: none !important;
		height: 100% !important;
		max-height: none !important;
		border: 0 !important;
		border-radius: 0 !important;
		box-shadow: none !important;
		background: transparent !important;
		font-family: inherit !important;
	}

	.legacy-shell :global(.shortcuts-backdrop) {
		display: none !important;
	}

	.legacy-shell :global(.modal-header),
	.legacy-shell :global(.overlay-header),
	.legacy-shell :global(.card-header),
	.legacy-shell :global(.panel-header),
	.legacy-shell :global(.debug-header) {
		background: rgba(235, 216, 178, 0.28) !important;
		border-color: rgba(71, 61, 46, 0.35) !important;
	}

	.legacy-shell :global(.map-embed > .close-btn) {
		display: none !important;
	}

	.legacy-shell :global(.debug-header) {
		display: none !important;
	}

	.legacy-shell :global(.map-embed) {
		position: absolute !important;
		inset: 0 !important;
		z-index: 1 !important;
		background: #d7c7a8 !important;
	}

	.legacy-shell :global(.debug-panel),
	.legacy-shell :global(.debug-panel.left-dock) {
		position: static !important;
		width: 100% !important;
		height: 100% !important;
		border: 0 !important;
		background: transparent !important;
		font-family: inherit !important;
	}

	.legacy-shell :global(.focail-panel),
	.legacy-shell :global(.chat-panel) {
		height: 100% !important;
		background: transparent !important;
		font-family: inherit !important;
	}

	.legacy-shell :global(.focail-panel .panel-header) {
		display: none !important;
	}

	@media (max-width: 760px) {
		.notebook-overlay-frame.drawer,
		.notebook-overlay-frame.modal,
		.notebook-overlay-frame.utility {
			inset: 3.2rem 0.55rem 0.65rem;
			width: auto;
			height: auto;
			max-height: none;
		}

		.notebook-overlay-header {
			padding: 0.62rem 0.75rem 0.5rem 1.25rem;
		}

		.eyebrow {
			font-size: 0.58rem;
		}

		.utility-grid {
			grid-template-columns: 1fr;
		}

		.notebook-overlay-body {
			margin-left: 0.8rem;
		}
	}
</style>
