<script lang="ts">
	import { languageHints, nameHints, uiConfig, npcsHere } from '../stores/game';
	import MoodIcon from './MoodIcon.svelte';

	let { onclose }: { onclose?: () => void } = $props();

	const approvedPortraits = new Map([
		['Padraig Darcy', 'padraig-darcy'],
		['Siobhan Murphy', 'siobhan-murphy'],
		['Fr. Declan Tierney', 'fr-declan-tierney'],
		['Roisin Connolly', 'roisin-connolly'],
		["Tommy O'Brien", 'tommy-o-brien'],
		['Aoife Brennan', 'aoife-brennan'],
		['Mick Flanagan', 'mick-flanagan'],
		['Niamh Darcy', 'niamh-darcy'],
		['Seamus Gallagher', 'seamus-gallagher'],
		['Maire Gallagher', 'maire-gallagher'],
		['Colm Gallagher', 'colm-gallagher'],
		['Cormac Duffy', 'cormac-duffy'],
		['Nora Duffy', 'nora-duffy'],
		['Brendan Duffy', 'brendan-duffy'],
		['Eamon Walsh', 'eamon-walsh'],
		['Kathleen Walsh', 'kathleen-walsh'],
		['Ciaran Walsh', 'ciaran-walsh'],
		['Liam Murphy', 'liam-murphy'],
		['Brigid Ni Fhatharta', 'brigid-ni-fhatharta'],
		['Una Malone', 'una-malone'],
		['Sean Ruadh Kelly', 'sean-ruadh-kelly'],
		['Peig Hannigan', 'peig-hannigan'],
		['Martin Concannon', 'martin-concannon'],
	]);

	function portraitUrl(realName: string): string {
		const slug = approvedPortraits.get(realName) ?? 'unknown-neighbour';
		return `/rundale/notebook-ui/people/portrait-${slug}.png`;
	}

	function useFallbackPortrait(event: Event) {
		const image = event.currentTarget as HTMLImageElement;
		const fallback =
			'/rundale/notebook-ui/people/portrait-unknown-neighbour.png';
		if (!image.src.endsWith(fallback)) image.src = fallback;
	}
</script>

{#snippet hintList()}
	{#if $nameHints.length > 0 || $languageHints.length > 0}
		<ul class="hint-list">
			{#each $nameHints as hint, i (hint.word + '#' + i)}
				<li class="hint-item name-hint hint-name">
					<span class="word">{hint.word}</span>
					<span class="pronunciation">[{hint.pronunciation}]</span>
					{#if hint.meaning}
						<span class="meaning">— {hint.meaning}</span>
					{/if}
				</li>
			{/each}
			{#each $languageHints as hint, i (hint.word + '#' + i)}
				<li class="hint-item hint-irish">
					<span class="word">{hint.word}</span>
					<span class="pronunciation">[{hint.pronunciation}]</span>
					{#if hint.meaning}
						<span class="meaning">— {hint.meaning}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{:else}
		<p class="empty">No words yet.</p>
	{/if}
{/snippet}

{#snippet npcList()}
	{#if $npcsHere.length > 0}
		<ul class="npc-list" data-testid="npcs-present">
			{#each $npcsHere as npc (npc.real_name)}
				<li class="npc-item">
					<img
						class="npc-portrait"
						src={portraitUrl(npc.real_name)}
						alt=""
						aria-hidden="true"
						loading="lazy"
						onerror={useFallbackPortrait}
					/>
					<span class="npc-mood"
						><MoodIcon mood={npc.mood} emoji={npc.mood_emoji} /></span
					>
					<span class="npc-copy">
						<span class="npc-name">{npc.name}</span>
						{#if npc.introduced && npc.occupation}
							<span class="npc-occupation">{npc.occupation}</span>
						{/if}
					</span>
				</li>
			{/each}
		</ul>
	{:else}
		<p class="empty">No one is about.</p>
	{/if}
{/snippet}

{#if onclose}
	<div class="focail-panel" data-testid="mobile-people-panel">
		<div class="panel-header">
			<span class="panel-title"
				><span class="panel-title-word">Focail Gaeilge</span>
				<span class="panel-title-label">(Irish Words)</span></span
			>
			<button
				type="button"
				class="close-btn"
				aria-label="Close Irish words panel"
				title="Close"
				onclick={onclose}><span aria-hidden="true">&times;</span></button
			>
		</div>
		<div class="panel-content">
			<section aria-labelledby="mobile-present-heading">
				<h3 id="mobile-present-heading">Present</h3>
				{@render npcList()}
			</section>
			<section aria-labelledby="mobile-words-heading">
				<h3 id="mobile-words-heading">{$uiConfig.hints_label}</h3>
				{@render hintList()}
			</section>
		</div>
	</div>
{:else}
	<aside class="sidebar" data-testid="sidebar">
		<details open>
			<summary>Present</summary>
			{@render npcList()}
		</details>
		<details open>
			<summary>{$uiConfig.hints_label}</summary>
			{@render hintList()}
		</details>
	</aside>
{/if}

<style>
	.focail-panel {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--color-panel-bg);
	}

	.panel-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.panel-title {
		flex: 1;
		display: flex;
		align-items: baseline;
		gap: 0.4em;
	}

	.panel-title-word {
		color: var(--color-accent);
		font-weight: 600;
		font-style: italic;
		font-size: 0.85rem;
	}

	.panel-title-label {
		font-family: var(--font-display);
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.13em;
		color: var(--color-muted);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-muted);
		font-size: 1.4rem;
		cursor: pointer;
		padding: 0 4px;
		line-height: 1;
	}

	.close-btn:hover,
	.close-btn:focus-visible {
		color: var(--color-fg);
	}

	.close-btn:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 2px;
		border-radius: 2px;
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
	}

	.panel-content section + section {
		border-top: 1px solid var(--color-border);
	}

	.panel-content h3 {
		margin: 0;
		padding: 0.55rem 0.75rem 0.2rem;
		color: var(--color-muted);
		font: 600 0.62rem/1.2 var(--font-display);
		letter-spacing: 0.13em;
		text-transform: uppercase;
	}

	.sidebar {
		background: var(--color-panel-bg);
		border-left: 1px solid var(--color-border);
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		min-height: 0;
	}

	details {
		border-bottom: 1px solid var(--color-border);
	}

	summary {
		padding: 0.55rem 0.75rem;
		font-family: var(--font-display);
		font-size: 0.62rem;
		text-transform: uppercase;
		letter-spacing: 0.13em;
		color: var(--color-muted);
		cursor: pointer;
		user-select: none;
		list-style: none;
	}

	summary::-webkit-details-marker {
		display: none;
	}

	summary::before {
		content: '▸ ';
		font-size: 0.55rem;
		opacity: 0.7;
	}

	details[open] summary::before {
		content: '▾ ';
	}

	.npc-list {
		list-style: none;
		margin: 0;
		padding: 0.25rem 0;
	}

	.npc-portrait {
		width: 2.5rem;
		height: 2.5rem;
		flex: 0 0 auto;
		object-fit: cover;
		border: 1px solid color-mix(in srgb, var(--color-accent) 55%, transparent);
		border-radius: 50%;
		background: var(--color-input-bg);
	}

	.npc-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		font-size: 0.8rem;
	}

	.npc-item:last-child {
		border-bottom: none;
	}

	.npc-mood {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
	}

	.npc-copy {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.npc-name {
		color: var(--color-name);
		font-weight: 600;
	}

	.npc-occupation {
		color: var(--color-muted);
		font-size: 0.72rem;
	}

	.hint-list {
		list-style: none;
		margin: 0;
		padding: 0.25rem 0;
	}

	.hint-item {
		padding: 0.4rem 0.75rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem;
		align-items: baseline;
		border-bottom: 1px solid var(--color-border);
		font-size: 0.8rem;
	}

	.hint-item:last-child {
		border-bottom: none;
	}

	.word {
		font-weight: 600;
		font-style: italic;
	}

	.hint-irish .word {
		color: var(--color-irish);
	}

	.hint-name .word {
		color: var(--color-name);
		font-style: normal;
	}

	.pronunciation {
		color: var(--color-muted);
	}

	.meaning {
		color: var(--color-fg);
		font-size: 0.75rem;
	}

	.empty {
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.8rem;
		padding: 0.5rem 0.75rem;
		margin: 0;
	}
</style>
