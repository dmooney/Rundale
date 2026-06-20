<script lang="ts">
	import type { SceneHotspotView } from '$lib/types';

	let {
		hotspots,
		debug = false,
		disabled = false,
		onActivate,
	}: {
		hotspots: SceneHotspotView[];
		debug?: boolean;
		disabled?: boolean;
		onActivate: (hotspot: SceneHotspotView) => void;
	} = $props();

	function rectStyle(hotspot: SceneHotspotView): string | null {
		if (!('rect' in hotspot.shape)) return null;
		const [x, y, w, h] = hotspot.shape.rect;
		return `left:${x}%;top:${y}%;width:${w}%;height:${h}%;`;
	}
</script>

<div class="hotspot-layer" aria-label="Scene hotspots">
	{#each hotspots as hotspot (hotspot.id)}
		{@const style = rectStyle(hotspot)}
		{#if style}
			<button
				type="button"
				class="hotspot"
				class:debug
				style={style}
				disabled={disabled}
				aria-label={hotspot.label}
				title={hotspot.label}
				onclick={() => onActivate(hotspot)}
			>
				{#if debug}
					<span>{hotspot.label}</span>
				{/if}
			</button>
		{/if}
	{/each}
</div>

<style>
	.hotspot-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.hotspot {
		position: absolute;
		display: block;
		padding: 0;
		border: 1px solid transparent;
		border-radius: 2px;
		background: transparent;
		cursor: pointer;
		pointer-events: auto;
		transition: border-color 0.12s ease, background 0.12s ease;
	}

	.hotspot:hover:not(:disabled),
	.hotspot:focus-visible:not(:disabled) {
		border-color: color-mix(in srgb, var(--color-accent) 75%, white);
		background: color-mix(in srgb, var(--color-accent) 16%, transparent);
		outline: none;
	}

	.hotspot.debug {
		border-color: color-mix(in srgb, var(--color-accent) 62%, transparent);
		background: color-mix(in srgb, var(--color-accent) 12%, transparent);
	}

	.hotspot:disabled {
		cursor: not-allowed;
	}

	.hotspot span {
		position: absolute;
		left: 0.2rem;
		top: 0.2rem;
		max-width: calc(100% - 0.4rem);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-bg);
		background: color-mix(in srgb, var(--color-accent) 86%, black);
		border-radius: 2px;
		padding: 0.1rem 0.25rem;
		font: 600 0.58rem/1.1 var(--font-display);
	}
</style>
