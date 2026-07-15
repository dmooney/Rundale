<script lang="ts">
	import type { NpcInfo } from '$lib/types';
	import { isNotebookLogEntry } from '$lib/notebook/log';
	import { notebookPersonLabel } from '$lib/notebook/people';
	import { textLog, streamingActive, loadingPhrase, loadingColor } from '../../stores/game';

	let { selectedNpc }: { selectedNpc: NpcInfo | null } = $props();

	const journalEntries = $derived(
		$textLog
			.filter(
				(entry) =>
					entry.source !== 'system' &&
					entry.subtype !== 'location' &&
					isNotebookLogEntry(entry),
			)
			.slice(-4),
	);

	const sceneSigns = [
		{ label: 'Chapel Lane', className: 'chapel' },
		{ label: 'Shop Road', className: 'shop' },
		{ label: 'Bridge', className: 'bridge' }
	];
</script>

<section class="world-stage" aria-label="Parish scene">
	{#each sceneSigns as sign (sign.label)}
		<span class={`scene-sign ${sign.className}`}>{sign.label}</span>
	{/each}

	<div class="annotation selected-person" class:hidden={!selectedNpc}>
		<span class="pin" aria-hidden="true"></span>
		{#if selectedNpc}
			<span>{notebookPersonLabel(selectedNpc)}</span>
		{/if}
	</div>

	{#if $streamingActive}
		<div class="working-note" role="status">
			<span class="spinner" style="border-color: rgba({$loadingColor[0]}, {$loadingColor[1]}, {$loadingColor[2]}, 0.35); border-top-color: rgb({$loadingColor[0]}, {$loadingColor[1]}, {$loadingColor[2]});"></span>
			<span>{$loadingPhrase || 'Ink drying...'}</span>
		</div>
	{/if}

	{#if journalEntries.length > 0}
		<div class="journal-strip" aria-label="Recent journal">
			{#each journalEntries as entry, index (entry.id || `${entry.source}:${index}:${entry.content}`)}
				<article class="journal-entry" class:player={entry.source === 'player'} class:system={entry.source === 'system'}>
					<span>{entry.source === 'player' ? 'You' : entry.source}</span>
					<p>{entry.content}</p>
				</article>
			{/each}
		</div>
	{/if}
</section>

<style>
	.world-stage {
		position: absolute;
		inset: 0;
		z-index: 2;
		pointer-events: none;
	}

	.annotation,
	.working-note {
		position: absolute;
		z-index: 4;
		background: rgba(244, 226, 184, 0.88);
		border: 1px solid rgba(70, 49, 24, 0.25);
		box-shadow: 0 5px 14px rgba(35, 25, 11, 0.16);
		color: var(--notebook-ink);
		backdrop-filter: blur(2px);
	}

	.selected-person {
		left: 47%;
		top: 48%;
		display: flex;
		align-items: center;
		gap: 0.35rem;
		padding: 0.3rem 0.55rem;
		border-radius: 999px;
		font-size: 0.78rem;
		font-style: italic;
		transform: rotate(-2deg);
	}

	.scene-sign {
		position: absolute;
		z-index: 3;
		padding: 0.35rem 1rem;
		background: url('/notebook-ui/assets/paper-card.svg') center / 100% 100%;
		color: var(--notebook-ink);
		font-family: var(--font-body);
		font-size: clamp(0.82rem, 1vw, 1.08rem);
		font-style: italic;
		text-shadow: 0 1px rgba(255, 248, 226, 0.6);
		filter: drop-shadow(0 2px 3px rgba(22, 17, 10, 0.25));
	}

	.chapel {
		left: 13%;
		top: 14%;
		transform: rotate(-2deg);
	}

	.shop {
		right: 27%;
		top: 37%;
		transform: rotate(2deg);
	}

	.bridge {
		right: 18%;
		bottom: 28%;
		transform: rotate(-3deg);
	}

	.hidden {
		display: none;
	}

	.pin {
		width: 0.65rem;
		aspect-ratio: 1;
		border: 2px solid var(--color-accent);
		border-radius: 50%;
		box-shadow: 0 0 0 0.18rem rgba(255, 255, 255, 0.45);
	}

	.working-note {
		right: 1rem;
		bottom: 1rem;
		display: flex;
		align-items: center;
		gap: 0.45rem;
		padding: 0.45rem 0.65rem;
		border-radius: 999px;
		font-size: 0.78rem;
		color: var(--notebook-ink-soft);
	}

	.spinner {
		width: 1rem;
		aspect-ratio: 1;
		border: 2px solid;
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.journal-strip {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 0.5rem;
		position: absolute;
		left: 25%;
		right: 31%;
		bottom: 5.9rem;
		z-index: 4;
	}

	.journal-entry {
		min-width: 0;
		padding: 0.55rem 0.65rem;
		border: 1px solid rgba(70, 49, 24, 0.22);
		border-radius: 0.3rem;
		background: rgba(255, 251, 232, 0.75);
		box-shadow: 0 6px 16px rgba(36, 26, 12, 0.1);
	}

	.journal-entry span {
		display: block;
		margin-bottom: 0.15rem;
		color: var(--color-accent);
		font-family: var(--font-display);
		font-size: 0.62rem;
		letter-spacing: 0.09em;
		text-transform: uppercase;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.journal-entry p {
		display: -webkit-box;
		line-clamp: 2;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		margin: 0;
		color: var(--notebook-ink);
		font-size: 0.82rem;
		line-height: 1.35;
	}

	.journal-entry.player {
		background: rgba(255, 239, 193, 0.74);
	}

	.journal-entry.system span {
		color: var(--notebook-ink-soft);
	}

	@media (max-width: 1100px) {
		.journal-strip {
			left: 1rem;
			right: 1rem;
			max-width: none;
		}
	}

	@media (max-width: 700px) {
		.world-stage {
			position: relative;
			display: block;
			height: 28rem;
			inset: auto;
		}

		.journal-strip {
			grid-template-columns: 1fr;
			bottom: 6rem;
		}
	}
</style>
