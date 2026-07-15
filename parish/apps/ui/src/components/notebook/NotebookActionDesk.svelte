<script lang="ts">
	import type { NpcInfo } from '$lib/types';
	import { fullMapOpen, requestIntentDraft } from '../../stores/game';
	import InputField from '../InputField.svelte';
	import { notebookActionDraft, type NotebookAction } from '$lib/notebook/actions';

	let { selectedNpc }: { selectedNpc: NpcInfo | null } = $props();

	const actions: Array<{ id: NotebookAction; label: string; icon: string }> = [
		{ id: 'talk', label: 'Talk', icon: '/notebook-ui/assets/icon-talk.svg' },
		{ id: 'ask', label: 'Ask', icon: '/notebook-ui/assets/icon-ask.svg' },
		{ id: 'help', label: 'Help', icon: '/notebook-ui/assets/icon-help.svg' },
		{ id: 'observe', label: 'Observe', icon: '/notebook-ui/assets/icon-observe.svg' },
		{ id: 'leave', label: 'Leave', icon: '/notebook-ui/assets/icon-leave.svg' },
	];

	function seed(action: NotebookAction) {
		requestIntentDraft(notebookActionDraft(action, selectedNpc));
	}
</script>

<section class="action-desk" aria-label="Notebook actions">
	<div class="desk-card map-card">
		<button type="button" onclick={() => fullMapOpen.set(true)} aria-label="Open map">
			<img src="/notebook-ui/assets/icon-map.svg" alt="" aria-hidden="true" />
			<span>Map</span>
		</button>
	</div>

	<div class="desk-card time-card" aria-label="Time speed">
		<img src="/notebook-ui/assets/icon-time.svg" alt="" aria-hidden="true" />
		<span>Time</span>
		<strong>x1</strong>
	</div>

	<div class="stamp-row">
		{#each actions as action (action.id)}
			<button
				type="button"
				class="stamp"
				onclick={() => seed(action.id)}
				aria-label={`${action.label}${selectedNpc ? ` ${selectedNpc.name}` : ''}`}
			>
				<img src={action.icon} alt="" aria-hidden="true" />
				<strong>{action.label}</strong>
			</button>
		{/each}
	</div>

	<div class="intent-strip">
		<span class="intent-label">Intent</span>
		<div class="input-pocket">
			<InputField autoFocus={false} />
		</div>
	</div>

	<div class="active-card" aria-label="Active intents">
		<span>Active Intents</span>
		<p>(none)</p>
		<div class="ink-line"></div>
	</div>
</section>

<style>
	.action-desk {
		position: absolute;
		inset: auto 0 0 0;
		z-index: 4;
		min-height: clamp(8.7rem, 17vh, 11rem);
		pointer-events: none;
	}

	.stamp-row,
	.intent-strip {
		pointer-events: auto;
	}

	.stamp-row {
		position: absolute;
		left: 50%;
		bottom: clamp(4.9rem, 8.5vh, 5.8rem);
		transform: translateX(-50%);
		display: flex;
		align-items: flex-end;
		justify-content: center;
		filter: drop-shadow(0 8px 12px rgba(22, 16, 9, 0.28));
	}

	.stamp {
		display: grid;
		place-items: center;
		gap: 0.08rem;
		width: clamp(4.8rem, 6.4vw, 6.2rem);
		height: clamp(3.8rem, 7.4vh, 4.8rem);
		border: 0;
		background: url('/notebook-ui/assets/action-card.svg') center / 100% 100%;
		color: var(--notebook-ink);
		cursor: pointer;
	}

	.stamp:hover,
	.stamp:focus-visible {
		color: color-mix(in srgb, var(--color-accent) 55%, var(--notebook-ink));
		filter: saturate(1.12) brightness(1.04);
	}

	.stamp img {
		width: clamp(1.65rem, 2.2vw, 2.2rem);
		height: clamp(1.65rem, 2.2vw, 2.2rem);
	}

	.stamp strong {
		font-family: var(--font-body);
		font-size: 0.82rem;
		font-style: italic;
		font-weight: 500;
	}

	.intent-strip {
		display: grid;
		grid-template-columns: auto minmax(14rem, 1fr);
		align-items: stretch;
		gap: 0.7rem;
		position: absolute;
		left: 50%;
		bottom: 0.9rem;
		width: min(48rem, 49vw);
		min-height: clamp(4.2rem, 7.6vh, 5.3rem);
		padding: 1rem 2.6rem 1rem 2.1rem;
		transform: translateX(-50%);
		background: url('/notebook-ui/assets/intent-slip.svg') center / 100% 100%;
		filter: drop-shadow(0 10px 16px rgba(22, 16, 9, 0.28));
	}

	.intent-label {
		align-self: center;
		padding: 0 0.45rem;
		font-family: var(--font-body);
		font-size: 1.05rem;
		font-style: italic;
		color: var(--notebook-ink);
	}

	.input-pocket {
		min-width: 0;
	}

	.input-pocket :global(.input-wrapper) {
		position: relative;
		bottom: auto;
		z-index: auto;
		padding: 0;
	}

	.input-pocket :global(.npc-chips),
	.input-pocket :global(.travel-chips) {
		display: none;
	}

	.input-pocket :global(.input-form) {
		padding: 0;
		border: 0;
		background: transparent;
	}

	.input-pocket :global(.input-field) {
		min-height: 2.35rem;
		border-color: rgba(64, 44, 19, 0.24);
		border-radius: 0.2rem;
		background:
			linear-gradient(180deg, rgba(255, 255, 255, 0.26), transparent),
			rgba(255, 252, 236, 0.74);
		color: var(--notebook-ink);
	}

	.input-pocket :global(.send-btn) {
		border-radius: 999px;
		background: transparent;
		color: var(--notebook-ink-soft);
		border-color: transparent;
	}

	.desk-card,
	.active-card {
		pointer-events: auto;
		position: absolute;
		background: url('/notebook-ui/assets/card-small.svg') center / 100% 100%;
		filter: drop-shadow(0 9px 14px rgba(22, 16, 9, 0.32));
		color: var(--notebook-ink);
	}

	.desk-card {
		left: 0.3rem;
		bottom: 0.35rem;
		width: clamp(6.8rem, 8vw, 8.5rem);
		height: clamp(5.8rem, 11vh, 7.3rem);
		display: grid;
		place-items: center;
	}

	.time-card {
		left: clamp(7rem, 8.5vw, 9rem);
		gap: 0.1rem;
		font-family: var(--font-body);
		font-style: italic;
		text-align: center;
	}

	.desk-card button {
		all: unset;
		display: grid;
		place-items: center;
		gap: 0.2rem;
		cursor: pointer;
	}

	.desk-card img {
		width: 2.25rem;
		height: 2.25rem;
	}

	.desk-card span,
	.active-card span {
		font-family: var(--font-body);
		font-size: 1rem;
		font-style: italic;
	}

	.time-card strong {
		font-family: var(--font-display);
		font-size: 1rem;
		font-weight: 400;
	}

	.active-card {
		right: 0.85rem;
		bottom: 0.35rem;
		width: clamp(15rem, 21vw, 22rem);
		height: clamp(5.7rem, 11vh, 7.1rem);
		padding: 1.1rem 1.4rem;
	}

	.active-card p {
		margin: 0.45rem 0 0;
		font-size: 0.86rem;
		font-style: italic;
	}

	.ink-line {
		margin-top: 0.45rem;
		border-bottom: 1px dashed rgba(43, 33, 20, 0.52);
	}

	@media (max-width: 900px) {
		.action-desk {
			position: relative;
			min-height: 16rem;
			padding: 0 0.75rem 0.75rem;
		}

		.stamp-row {
			position: relative;
			left: auto;
			bottom: auto;
			transform: none;
			width: 100%;
			padding: 0 0.25rem;
		}

		.stamp {
			width: 20%;
			min-width: 0;
		}

		.stamp strong {
			font-size: 0.72rem;
		}

		.intent-strip {
			position: relative;
			left: auto;
			bottom: auto;
			width: 100%;
			transform: none;
			grid-template-columns: 1fr;
			gap: 0.35rem;
			margin-top: 0.45rem;
		}

		.desk-card,
		.active-card {
			display: none;
		}
	}
</style>
