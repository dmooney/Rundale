<script lang="ts">
	import type { SceneNpcView } from '$lib/types';

	let {
		npcs,
		disabled = false,
		onActivate,
	}: {
		npcs: SceneNpcView[];
		disabled?: boolean;
		onActivate: (npc: SceneNpcView) => void;
	} = $props();

	function npcStyle(npc: SceneNpcView): string {
		const flip = npc.flip ? ' scaleX(-1)' : '';
		return `left:${npc.x}%;bottom:${100 - npc.y}%;transform:translate(-50%, 0) scale(${npc.scale})${flip};`;
	}
</script>

<div class="npc-sprite-layer" aria-label="People in this scene">
	{#each npcs as npc (npc.npc_id)}
		<button
			type="button"
			class="npc-sprite"
			style={npcStyle(npc)}
			disabled={disabled}
			aria-label="Speak to {npc.display_name}"
			title={npc.display_name}
			onclick={() => onActivate(npc)}
		>
			{#if npc.sprite_url}
				<img src={npc.sprite_url} alt="" draggable="false" />
			{:else}
				<span class="sprite-fallback" aria-hidden="true"></span>
			{/if}
			<span class="npc-tooltip">
				<span>{npc.display_name}</span>
				<span class="npc-mood">{npc.mood_emoji} {npc.mood}</span>
			</span>
		</button>
	{/each}
</div>

<style>
	.npc-sprite-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.npc-sprite {
		position: absolute;
		width: clamp(2.1rem, 7.5vw, 3.6rem);
		aspect-ratio: 2 / 3;
		padding: 0;
		border: 0;
		background: transparent;
		cursor: pointer;
		transform-origin: 50% 100%;
		pointer-events: auto;
		filter: drop-shadow(0 0.28rem 0.14rem rgba(0, 0, 0, 0.38));
	}

	.npc-sprite:disabled {
		cursor: not-allowed;
		opacity: 0.65;
	}

	.npc-sprite img,
	.sprite-fallback {
		display: block;
		width: 100%;
		height: 100%;
		object-fit: contain;
		image-rendering: pixelated;
	}

	.sprite-fallback {
		border-radius: 50% 50% 42% 42%;
		background:
			linear-gradient(
				180deg,
				color-mix(in srgb, var(--color-accent) 70%, white),
				color-mix(in srgb, var(--color-accent) 50%, black)
			);
		border: 1px solid rgba(0, 0, 0, 0.35);
	}

	.npc-sprite:hover:not(:disabled),
	.npc-sprite:focus-visible:not(:disabled) {
		outline: none;
		filter:
			drop-shadow(0 0.28rem 0.14rem rgba(0, 0, 0, 0.38))
			drop-shadow(0 0 0.45rem var(--color-accent));
	}

	.npc-tooltip {
		position: absolute;
		left: 50%;
		bottom: calc(100% + 0.3rem);
		transform: translateX(-50%);
		display: none;
		min-width: max-content;
		max-width: min(14rem, 56vw);
		padding: 0.25rem 0.4rem;
		border: 1px solid var(--color-border);
		border-radius: 3px;
		background: color-mix(in srgb, var(--color-panel-bg) 96%, black);
		color: var(--color-fg);
		font: 600 0.68rem/1.15 var(--font-display);
		text-align: center;
		box-shadow: 0 0.45rem 1rem rgba(0, 0, 0, 0.28);
		z-index: 4;
	}

	.npc-mood {
		display: block;
		margin-top: 0.15rem;
		color: var(--color-muted);
		font: 400 0.62rem/1.1 var(--font-body);
	}

	.npc-sprite:hover .npc-tooltip,
	.npc-sprite:focus-visible .npc-tooltip {
		display: block;
	}
</style>
