<script lang="ts">
	import type { SceneNpcView } from '$lib/types';

	interface Props {
		npcs: SceneNpcView[];
		onSpriteClick: (npc: SceneNpcView) => void;
	}

	let { npcs, onSpriteClick }: Props = $props();

	/**
	 * Tooltip text for a sprite: display_name always.
	 * real_name is intentionally not used when introduced is false — the diorama
	 * must never reveal a name the backend withheld (AC-6).
	 */
	function tooltipText(npc: SceneNpcView): string {
		return npc.display_name;
	}

	function handleKeydown(e: KeyboardEvent, npc: SceneNpcView) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onSpriteClick(npc);
		}
	}
</script>

<div class="sprite-layer" aria-label="NPC sprites">
	{#each npcs as npc (npc.id)}
		<!-- Foot-anchored: left={x}%, bottom={100 - y}% positions the foot of the
		     sprite. translateX(-50%) centres it on the anchor point. -->
		<button
			class="sprite-btn"
			style="left: {npc.x}%; bottom: {100 - npc.y}%;"
			title={tooltipText(npc)}
			aria-label="Speak to {tooltipText(npc)}"
			onclick={() => onSpriteClick(npc)}
			onkeydown={(e) => handleKeydown(e, npc)}
			type="button"
		>
			<img
				class="sprite-img"
				src={npc.sprite_url}
				alt={tooltipText(npc)}
				style="
					transform: translateX(-50%) {npc.flip ? 'scaleX(-1)' : ''};
					scale: {npc.scale};
					image-rendering: pixelated;
				"
				draggable="false"
			/>
		</button>
	{/each}
</div>

<style>
	.sprite-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.sprite-btn {
		position: absolute;
		background: none;
		border: none;
		padding: 0;
		margin: 0;
		cursor: pointer;
		pointer-events: all;
		transform-origin: bottom center;
		/* No translateX here — the img handles it so the button's
		   clickable area stays anchored at the foot point. */
	}

	.sprite-btn:focus-visible .sprite-img {
		outline: 2px solid var(--color-accent, #d4a44a);
		outline-offset: 2px;
	}

	.sprite-btn:hover .sprite-img {
		filter: brightness(1.1);
	}

	.sprite-img {
		display: block;
		/* Actual dimensions come from the image itself (48px wide sprites) */
		height: auto;
	}
</style>
