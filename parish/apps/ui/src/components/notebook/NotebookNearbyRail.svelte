<script lang="ts">
	import type { NpcInfo } from '$lib/types';
	import { notebookPersonInitial, notebookPersonLabel } from '$lib/notebook/people';
	import MoodIcon from '../MoodIcon.svelte';

	let {
		npcs,
		selectedRealName,
		onselect,
	}: {
		npcs: NpcInfo[];
		selectedRealName: string | null;
		onselect: (realName: string) => void;
	} = $props();
</script>

<aside class="nearby-rail" aria-label="People nearby">
	<div class="rail-card">
		<header>
			<span class="rail-title">Nearby</span>
			<span class="rail-rule"></span>
		</header>

		{#if npcs.length > 0}
			<ul class="nearby-list">
				{#each npcs as npc (npc.real_name)}
					<li>
						<button
							type="button"
							title={npc.name}
							class:selected={npc.real_name === selectedRealName}
							aria-pressed={npc.real_name === selectedRealName}
							onclick={() => onselect(npc.real_name)}
						>
							<span class="portrait" aria-hidden="true">
								<span>{notebookPersonInitial(npc)}</span>
							</span>
							<span class="person-copy">
								<span class="person-name">{notebookPersonLabel(npc)}</span>
								{#if npc.introduced && npc.occupation}
									<span class="person-role">{npc.occupation}</span>
								{:else}
									<span class="person-role">not yet introduced</span>
								{/if}
								<span class="mood-line">
									<MoodIcon mood={npc.mood} emoji={npc.mood_emoji} />
									<span>{npc.mood}</span>
								</span>
							</span>
							<span class="watch-mark" title="Aware of the scene" aria-label="Aware">○</span>
						</button>
					</li>
				{/each}
			</ul>
		{:else}
			<p class="empty">No one is close enough to mark in the margin.</p>
		{/if}
	</div>
</aside>

<style>
	.nearby-rail {
		position: absolute;
		left: 0.15rem;
		top: clamp(11rem, 21vh, 14rem);
		bottom: clamp(9.5rem, 20vh, 12.5rem);
		z-index: 6;
		min-width: 0;
		display: flex;
	}

	.rail-card {
		position: relative;
		width: clamp(8.9rem, 10vw, 10.4rem);
		height: 100%;
		margin: 0;
		padding: 1.35rem 0.7rem 1rem;
		background: url('/notebook-ui/assets/left-rail.svg') center / 100% 100%;
		filter: drop-shadow(0 10px 18px rgba(23, 17, 10, 0.32));
		overflow: hidden;
	}

	.rail-card::before {
		display: none;
	}

	header {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		padding: 0 0.35rem 0.65rem;
	}

	.rail-title {
		font-family: var(--font-body);
		font-style: italic;
		font-size: 1.05rem;
		color: var(--notebook-ink);
	}

	.rail-rule {
		flex: 1;
		border-top: 1px solid rgba(78, 54, 24, 0.25);
	}

	.nearby-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.62rem;
		max-height: calc(100% - 2.2rem);
		overflow: auto;
	}

	li {
		min-width: 0;
	}

	button {
		width: 100%;
		display: grid;
		grid-template-columns: 3.1rem minmax(0, 1fr) 0.8rem;
		gap: 0.35rem;
		align-items: center;
		padding: 0.1rem 0.15rem;
		border: 1px solid transparent;
		border-radius: 0.2rem;
		background: transparent;
		color: var(--notebook-ink);
		text-align: left;
		cursor: pointer;
	}

	button:hover,
	button:focus-visible,
	button.selected {
		background: rgba(255, 251, 232, 0.72);
		border-color: color-mix(in srgb, var(--color-accent) 35%, transparent);
	}

	.portrait {
		position: relative;
		width: 3rem;
		aspect-ratio: 0.84;
		display: grid;
		place-items: center;
		background: url('/notebook-ui/assets/portrait-slot.svg') center / contain no-repeat;
		color: var(--notebook-ink);
		font-family: var(--font-display);
		font-size: 1.25rem;
	}

	button.selected .portrait::after {
		content: '';
		position: absolute;
		inset: -0.35rem;
		background: url('/notebook-ui/assets/portrait-slot-selected.svg') center / contain no-repeat;
		pointer-events: none;
	}

	.person-copy {
		min-width: 0;
		display: grid;
		gap: 0.05rem;
	}

	.person-name,
	.person-role,
	.mood-line {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.person-name {
		color: var(--notebook-ink);
		font-weight: 650;
		font-size: 0.76rem;
	}

	.person-role {
		color: var(--notebook-ink-soft);
		font-size: 0.61rem;
	}

	.mood-line {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--notebook-ink-soft);
		font-size: 0.62rem;
	}

	.watch-mark {
		color: var(--color-accent);
		font-size: 0.82rem;
		opacity: 0.8;
	}

	.empty {
		margin: 0;
		padding: 0.5rem 0.45rem;
		color: var(--notebook-ink-soft);
		font-style: italic;
		font-size: 0.78rem;
	}

	@media (max-width: 900px) {
		.nearby-rail {
			position: relative;
			left: auto;
			top: auto;
			bottom: auto;
			order: 2;
			padding: 0.65rem 0.7rem 0;
		}

		.rail-card {
			width: auto;
			height: auto;
			max-width: none;
			min-height: 7.2rem;
			margin: 0;
			padding: 1.15rem 0.85rem 0.85rem;
		}

		.nearby-list {
			grid-auto-flow: column;
			grid-auto-columns: minmax(9rem, 1fr);
			overflow-x: auto;
		}
	}
</style>
