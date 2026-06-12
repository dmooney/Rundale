<script lang="ts">
	import type { SceneHotspotView, SceneAction } from '$lib/types';
	import { debugVisible } from '../../stores/debug';

	interface Props {
		hotspots: SceneHotspotView[];
		onAction: (action: SceneAction) => void;
	}

	let { hotspots, onAction }: Props = $props();

	function activate(action: SceneAction) {
		onAction(action);
	}

	function handleKeydown(e: KeyboardEvent, action: SceneAction) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			activate(action);
		}
	}

	/** Derive SVG element props from a hotspot shape. */
	function shapeProps(hotspot: SceneHotspotView): { type: 'rect'; x: number; y: number; w: number; h: number } | { type: 'polygon'; points: string } {
		const s = hotspot.shape;
		if ('rect' in s) {
			const [x, y, w, h] = s.rect;
			return { type: 'rect', x, y, w, h };
		}
		// polygon
		return {
			type: 'polygon',
			points: s.polygon.map(([px, py]) => `${px},${py}`).join(' ')
		};
	}
</script>

<!-- SVG over the full plate. 0 0 100 100 with preserveAspectRatio=none so
     percentage coords map 1-to-1 onto display pixels. -->
<svg
	class="hotspot-svg"
	viewBox="0 0 100 100"
	preserveAspectRatio="none"
	aria-label="Scene hotspots"
>
	{#each hotspots as hotspot (hotspot.id)}
		{@const sp = shapeProps(hotspot)}
		<!-- Focusable invisible overlay element per hotspot -->
		{#if sp.type === 'rect'}
			<rect
				class="hotspot"
				class:debug={$debugVisible}
				x={sp.x}
				y={sp.y}
				width={sp.w}
				height={sp.h}
				tabindex="0"
				role="button"
				aria-label={hotspot.label}
				onclick={() => activate(hotspot.action)}
				onkeydown={(e) => handleKeydown(e, hotspot.action)}
			/>
			{#if $debugVisible}
				<text
					class="debug-label"
					x={sp.x + sp.w / 2}
					y={sp.y + sp.h / 2}
					text-anchor="middle"
					dominant-baseline="middle"
				>{hotspot.label}</text>
			{/if}
		{:else}
			<polygon
				class="hotspot"
				class:debug={$debugVisible}
				points={sp.points}
				tabindex="0"
				role="button"
				aria-label={hotspot.label}
				onclick={() => activate(hotspot.action)}
				onkeydown={(e) => handleKeydown(e, hotspot.action)}
			/>
		{/if}
	{/each}
</svg>

<style>
	.hotspot-svg {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		overflow: visible;
	}

	.hotspot {
		fill: transparent;
		stroke: transparent;
		cursor: pointer;
		outline: none;
	}

	/* Debug / authoring overlay: hotspot outlines and labels become visible
	   when the debug panel is open (used during M5 hand-authoring). */
	.hotspot.debug {
		fill: rgba(255, 200, 0, 0.15);
		stroke: rgba(255, 200, 0, 0.8);
		stroke-width: 0.4;
	}

	.hotspot:focus-visible {
		fill: rgba(255, 255, 255, 0.12);
		stroke: rgba(255, 255, 255, 0.85);
		stroke-width: 0.5;
	}

	.hotspot:hover {
		fill: rgba(255, 255, 255, 0.08);
	}

	.debug-label {
		fill: rgba(255, 220, 0, 0.95);
		font-size: 3px;
		pointer-events: none;
		/* SVG text is in viewBox units (0–100), so this size renders readably */
	}
</style>
