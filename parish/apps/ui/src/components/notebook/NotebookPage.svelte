<script lang="ts">
	import type { NpcInfo, TextLogEntry } from '$lib/types';
	import { isNotebookLogEntry } from '$lib/notebook/log';
	import { notebookPersonInitial, notebookPersonLabel } from '$lib/notebook/people';
	import { languageHints, nameHints, npcsHere, textLog, worldState } from '../../stores/game';
	import MoodIcon from '../MoodIcon.svelte';
	import NotebookTabs from './NotebookTabs.svelte';

	let { selectedNpc }: { selectedNpc: NpcInfo | null } = $props();

	let activeTab = $state('notes');

	const recentNotes = $derived(
		$textLog
			.filter(
				(entry) =>
					entry.source !== 'player' &&
					entry.source !== 'system' &&
					entry.subtype !== 'location' &&
					isNotebookLogEntry(entry),
			)
			.slice(-5)
			.reverse(),
	);

	const hintFacts = $derived([...$nameHints, ...$languageHints].slice(-5).reverse());
	const nearbyWitnessCount = $derived(Math.max(0, $npcsHere.length - (selectedNpc ? 1 : 0)));

	function noteLabel(entry: TextLogEntry): string {
		if (entry.source === 'system') return 'place note';
		return entry.source;
	}
</script>

<aside class="notebook-page-wrap" aria-label="Notebook page">
	<section class="notebook-page" data-testid="notebook-page">
		<div class="binding" aria-hidden="true">
			{#each Array(9) as _, i (i)}
				<span></span>
			{/each}
		</div>

		{#if selectedNpc}
			<header class="person-header">
				<div class="sketch" aria-hidden="true">{notebookPersonInitial(selectedNpc)}</div>
				<div class="person-heading">
					<h2>{notebookPersonLabel(selectedNpc)}</h2>
					<div class="mood">
						<MoodIcon mood={selectedNpc.mood} emoji={selectedNpc.mood_emoji} />
						<span>{selectedNpc.mood}</span>
					</div>
					{#if selectedNpc.introduced && selectedNpc.occupation}
						<p>{selectedNpc.occupation}</p>
					{:else}
						<p>not yet properly introduced</p>
					{/if}
				</div>
			</header>

			{#if !selectedNpc.introduced}
				<p class="appearance-note">{selectedNpc.name}</p>
			{/if}

			<div class="section-rule"></div>

			<section class="page-section">
				<h3>Notes</h3>
				{#if recentNotes.length > 0}
					<ul class="note-list">
						{#each recentNotes.slice(0, 3) as note, i (note.id || `${note.source}:${i}`)}
							<li>
								<span>{noteLabel(note)}</span>
								<p>{note.content}</p>
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty">No direct notes about this person yet.</p>
				{/if}
			</section>

			<section class="page-section">
				<h3>Words & Names</h3>
				{#if hintFacts.length > 0}
					<ul class="fact-list">
						{#each hintFacts as hint, i (`${hint.word}:${i}`)}
							<li>
								<strong>{hint.word}</strong>
								{#if hint.meaning}
									<span>{hint.meaning}</span>
								{/if}
							</li>
						{/each}
					</ul>
				{:else}
					<p class="empty">The margin has no vocabulary notes yet.</p>
				{/if}
			</section>

			<section class="page-section">
				<h3>Witnesses Nearby</h3>
				<p class="empty">
					{nearbyWitnessCount === 0
						? 'No one else is close enough to put in the margin.'
						: `${nearbyWitnessCount} other ${nearbyWitnessCount === 1 ? 'person is' : 'people are'} close enough to notice.`}
				</p>
			</section>
		{:else}
			<header class="place-header">
				<span class="small-title">Place Notes</span>
				<h2>{$worldState?.location_name ?? 'Rundale'}</h2>
				<p>{$worldState?.location_description ?? 'The page is waiting for the parish to appear.'}</p>
			</header>

			<section class="page-section">
				<h3>Observe</h3>
				<p class="empty">Select someone nearby or write an intent in the desk below.</p>
			</section>
		{/if}
	</section>
	<NotebookTabs active={activeTab} onselect={(tab) => (activeTab = tab)} />
</aside>

<style>
	.notebook-page-wrap {
		position: absolute;
		right: clamp(0.7rem, 2vw, 2rem);
		top: clamp(5.3rem, 9vh, 6.2rem);
		bottom: clamp(12rem, 23vh, 15rem);
		z-index: 7;
		min-width: 0;
		display: flex;
		padding: 0;
	}

	.notebook-page {
		position: relative;
		width: clamp(19rem, 22vw, 23.5rem);
		height: 100%;
		min-height: 0;
		padding: 3.7rem 2.3rem 2rem 3.7rem;
		background: url('/notebook-ui/assets/notebook-page.svg') center / 100% 100%;
		filter: drop-shadow(0 18px 24px rgba(20, 15, 8, 0.36));
		color: var(--notebook-ink);
		overflow: hidden;
	}

	.notebook-page::after {
		display: none;
	}

	.binding {
		display: none;
	}

	.binding span {
		width: 0.55rem;
		height: 0.55rem;
		border-radius: 50%;
		border: 1px solid rgba(54, 38, 18, 0.35);
		background: rgba(77, 53, 23, 0.18);
		box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.18);
	}

	.person-header {
		display: grid;
		grid-template-columns: 4.4rem minmax(0, 1fr);
		gap: 0.8rem;
		align-items: start;
	}

	.sketch {
		display: grid;
		place-items: center;
		width: 4.4rem;
		aspect-ratio: 0.82;
		background: url('/notebook-ui/assets/portrait-slot.svg') center / contain no-repeat;
		font-family: var(--font-display);
		font-size: 2rem;
		color: rgba(65, 46, 21, 0.72);
	}

	h2,
	h3,
	p {
		margin: 0;
	}

	h2 {
		font-family: var(--font-body);
		font-size: clamp(1.35rem, 1.9vw, 1.82rem);
		font-style: italic;
		line-height: 1.05;
		font-weight: 500;
	}

	.person-heading {
		min-width: 0;
		display: grid;
		gap: 0.35rem;
	}

	.person-heading p,
	.place-header p,
	.empty {
		color: var(--notebook-ink-soft);
		font-size: 0.83rem;
		line-height: 1.45;
	}

	.mood {
		display: flex;
		align-items: center;
		gap: 0.35rem;
		color: color-mix(in srgb, #9a4f3e 72%, var(--notebook-ink));
		font-style: italic;
	}

	.section-rule {
		margin: 1rem 0 0.85rem;
		border-top: 1px solid rgba(75, 53, 24, 0.2);
	}

	.appearance-note {
		position: relative;
		z-index: 1;
		margin-top: 0.7rem;
		color: var(--notebook-ink-soft);
		font-size: 0.78rem;
		font-style: italic;
		line-height: 1.35;
	}

	.page-section {
		position: relative;
		z-index: 1;
		margin-top: 0.85rem;
	}

	h3,
	.small-title {
		display: block;
		margin-bottom: 0.35rem;
		font-family: var(--font-display);
		font-size: 0.62rem;
		letter-spacing: 0.13em;
		text-transform: uppercase;
		color: var(--notebook-ink-soft);
	}

	.note-list,
	.fact-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.45rem;
	}

	.note-list li {
		padding-left: 0.7rem;
		border-left: 2px solid rgba(153, 111, 48, 0.35);
	}

	.note-list span {
		display: block;
		color: var(--color-accent);
		font-family: var(--font-display);
		font-size: 0.58rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
	}

	.note-list p {
		display: -webkit-box;
		line-clamp: 3;
		-webkit-line-clamp: 3;
		-webkit-box-orient: vertical;
		overflow: hidden;
		color: var(--notebook-ink);
		font-size: 0.82rem;
		line-height: 1.38;
	}

	.fact-list li {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 0.45rem;
		align-items: baseline;
		color: var(--notebook-ink-soft);
		font-size: 0.82rem;
	}

	.fact-list strong {
		color: var(--notebook-ink);
		font-style: italic;
	}

	.place-header {
		display: grid;
		gap: 0.55rem;
	}

	@media (max-width: 900px) {
		.notebook-page-wrap {
			position: relative;
			right: auto;
			top: auto;
			bottom: auto;
			order: 3;
			padding: 0.7rem 0.7rem 1rem;
			flex-direction: column;
		}

		.notebook-page {
			width: 100%;
			max-width: none;
			min-width: 0;
			min-height: 26rem;
			padding: 3.6rem 2.2rem 2rem 3.4rem;
		}
	}
</style>
