<script lang="ts">
	import { mapData } from '../stores/game';
	import { submitInput } from '$lib/ipc';
	import { resolveLabels, distSq, estimateTextWidth, type EdgeLine } from '$lib/map-labels';
	import { projectWorld } from '$lib/map-projection';
	import type { MapLocation } from '$lib/types';
	import type { ProjectedLocation } from '$lib/map-projection';
	import type { ResolvedLabel } from '$lib/map-labels';

	interface Props {
		onclose: () => void;
	}

	let { onclose }: Props = $props();

	const NODE_R = 5;
	const PLAYER_R = 8;
	const LABEL_FONT_SIZE = 11;
	const MIN_ZOOM = 0.5;
	const MAX_ZOOM = 4;

	let zoom = $state(1);
	let panX = $state(0);
	let panY = $state(0);
	let dragging = $state(false);
	let lastPointer = $state({ x: 0, y: 0 });
	let tooltip: string | null = $state(null);

	let projected: ProjectedLocation[] = $derived(
		projectWorld($mapData?.locations ?? [])
	);

	// Compute bounding box of all projected locations for the SVG viewBox
	let bounds = $derived.by(() => {
		if (projected.length === 0) return { minX: 0, minY: 0, maxX: 800, maxY: 600 };
		const xs = projected.map((l) => l.x);
		const ys = projected.map((l) => l.y);
		const pad = 100;
		return {
			minX: Math.min(...xs) - pad,
			minY: Math.min(...ys) - pad,
			maxX: Math.max(...xs) + pad,
			maxY: Math.max(...ys) + pad
		};
	});

	let svgW = $derived(bounds.maxX - bounds.minX);
	let svgH = $derived(bounds.maxY - bounds.minY);

	// Locations in viewBox-local coords
	let localProjected: ProjectedLocation[] = $derived(
		projected.map((l) => ({
			...l,
			x: l.x - bounds.minX,
			y: l.y - bounds.minY
		}))
	);

	// O(1) location lookup map — avoids O(n) .find() per edge
	let locationMap: Map<string, ProjectedLocation> = $derived(
		new Map(localProjected.map((l) => [l.id, l]))
	);

	let fullEdgeLines: EdgeLine[] = $derived(
		($mapData?.edges ?? []).map(([src, dst]) => {
			const a = locationMap.get(src);
			const b = locationMap.get(dst);
			return a && b ? { x1: a.x, y1: a.y, x2: b.x, y2: b.y } : null;
		}).filter((e): e is EdgeLine => e !== null)
	);

	let labels: ResolvedLabel[] = $derived(
		resolveLabels(
			localProjected.map((loc) => ({
				nodeX: loc.x,
				nodeY: loc.y,
				nodeR: isPlayer(loc) ? PLAYER_R : NODE_R,
				textW: estimateTextWidth(loc.name, 20, LABEL_FONT_SIZE),
				textH: LABEL_FONT_SIZE
			})),
			svgW,
			svgH,
			fullEdgeLines
		)
	);

	function isPlayer(loc: MapLocation): boolean {
		return $mapData?.player_location === loc.id;
	}

	async function handleClick(loc: MapLocation) {
		if (!loc.adjacent) return;
		await submitInput(`go to ${loc.name}`);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === 'm' || e.key === 'M') {
			e.preventDefault();
			e.stopPropagation();
			onclose();
		}
	}

	function handleWheel(e: WheelEvent) {
		e.preventDefault();
		const delta = e.deltaY > 0 ? -0.1 : 0.1;
		zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoom + delta));
	}

	function handlePointerDown(e: PointerEvent) {
		dragging = true;
		lastPointer = { x: e.clientX, y: e.clientY };
		(e.currentTarget as Element).setPointerCapture(e.pointerId);
	}

	function handlePointerMove(e: PointerEvent) {
		if (!dragging) return;
		panX += e.clientX - lastPointer.x;
		panY += e.clientY - lastPointer.y;
		lastPointer = { x: e.clientX, y: e.clientY };
	}

	function handlePointerUp() {
		dragging = false;
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="overlay-backdrop" onclick={handleBackdropClick}>
	<div class="overlay-container">
		<div class="overlay-header">
			<span class="overlay-title">Parish Map</span>
			<span class="overlay-hint">Scroll to zoom &middot; Drag to pan &middot; Esc to close</span>
			<button class="close-btn" onclick={onclose} title="Close (Esc)">&times;</button>
		</div>
		<div
			class="map-viewport"
			onwheel={handleWheel}
			onpointerdown={handlePointerDown}
			onpointermove={handlePointerMove}
			onpointerup={handlePointerUp}
		>
			<svg
				viewBox="0 0 {svgW} {svgH}"
				xmlns="http://www.w3.org/2000/svg"
				role="img"
				aria-label="Full parish map"
				style="transform: translate({panX}px, {panY}px) scale({zoom}); transform-origin: center;"
			>
				<!-- Edges (reuse pre-computed fullEdgeLines for O(1) rendering) -->
				{#each fullEdgeLines as edge}
					<line x1={edge.x1} y1={edge.y1} x2={edge.x2} y2={edge.y2} class="edge" />
				{/each}

				<!-- Leader lines -->
				{#each localProjected as loc, i}
					{@const label = labels[i]}
					{@const r = isPlayer(loc) ? PLAYER_R : NODE_R}
					{@const threshold = (r + 8) ** 2}
					{#if label && distSq(label.cx, label.cy, loc.x, loc.y) > threshold}
						{@const angle = Math.atan2(label.cy - loc.y, label.cx - loc.x)}
						<line
							x1={loc.x + Math.cos(angle) * (r + 1)}
							y1={loc.y + Math.sin(angle) * (r + 1)}
							x2={label.cx - Math.cos(angle) * Math.min(label.w / 2, 8)}
							y2={label.cy - Math.sin(angle) * Math.min(label.h / 2, 6)}
							class="leader"
						/>
					{/if}
				{/each}

				<!-- Location nodes -->
				{#each localProjected as loc, i}
					{@const label = labels[i]}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<g
						class="node"
						class:player={isPlayer(loc)}
						class:adjacent={loc.adjacent}
						onclick={() => handleClick(loc)}
						onmouseenter={() => (tooltip = loc.name)}
						onmouseleave={() => (tooltip = null)}
					>
						<circle
							cx={loc.x}
							cy={loc.y}
							r={isPlayer(loc) ? PLAYER_R : NODE_R}
							class="node-circle"
						/>
						{#if label}
							<text
								x={label.cx}
								y={label.cy + LABEL_FONT_SIZE / 2 - 1}
								class="node-label"
							>
								{loc.name.length > 20 ? loc.name.slice(0, 18) + '\u2026' : loc.name}
							</text>
						{/if}
					</g>
				{/each}
			</svg>
		</div>
		{#if tooltip}
			<div class="tooltip">{tooltip}</div>
		{/if}
	</div>
</div>

<style>
	.overlay-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.overlay-container {
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		width: 90vw;
		max-width: 900px;
		height: 80vh;
		max-height: 700px;
		display: flex;
		flex-direction: column;
		position: relative;
		overflow: hidden;
	}

	.overlay-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.5rem 0.75rem;
		border-bottom: 1px solid var(--color-border);
		flex-shrink: 0;
	}

	.overlay-title {
		font-size: 0.9rem;
		font-weight: 600;
		color: var(--color-fg);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.overlay-hint {
		font-size: 0.7rem;
		color: var(--color-muted);
		flex: 1;
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

	.close-btn:hover {
		color: var(--color-fg);
	}

	.map-viewport {
		flex: 1;
		overflow: hidden;
		cursor: grab;
		user-select: none;
	}

	.map-viewport:active {
		cursor: grabbing;
	}

	svg {
		width: 100%;
		height: 100%;
		display: block;
	}

	.edge {
		stroke: var(--color-border);
		stroke-width: 1.5;
	}

	.leader {
		stroke: var(--color-muted);
		stroke-width: 0.4;
		stroke-dasharray: 2 1;
	}

	.node-circle {
		fill: var(--color-panel-bg);
		stroke: var(--color-muted);
		stroke-width: 1.5;
		cursor: default;
	}

	.node.adjacent .node-circle {
		stroke: var(--color-accent);
		cursor: pointer;
	}

	.node.adjacent .node-circle:hover {
		fill: var(--color-input-bg);
	}

	.node.player .node-circle {
		fill: var(--color-accent);
		stroke: var(--color-fg);
	}

	.node-label {
		font-size: 11px;
		fill: var(--color-muted);
		text-anchor: middle;
		pointer-events: none;
	}

	.node.player .node-label {
		fill: var(--color-fg);
		font-weight: 600;
	}

	.tooltip {
		position: absolute;
		bottom: 0.75rem;
		right: 0.75rem;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.25rem 0.6rem;
		font-size: 0.85rem;
		border-radius: 4px;
		pointer-events: none;
	}
</style>
