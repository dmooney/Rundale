<script lang="ts">
	import { mapData } from '../stores/game';
	import { fullMapOpen } from '../stores/game';
	import { submitInput } from '$lib/ipc';
	import { resolveLabels, distSq, estimateTextWidth, type EdgeLine } from '$lib/map-labels';
	import { projectWorld } from '$lib/map-projection';
	import type { MapLocation } from '$lib/types';
	import type { ProjectedLocation } from '$lib/map-projection';
	import type { ResolvedLabel } from '$lib/map-labels';
	import { tweened } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';

	/** Reference dimensions — visual sizes are authored relative to this. */
	const W = 320;
	const H = 240;
	/** Base sizes at the reference scale (W × H viewBox). */
	const BASE_NODE_R = 6;
	const BASE_PLAYER_R = 9;
	const BASE_FONT_SIZE = 28;
	/** Only show locations within this many hops on the minimap. */
	const MINIMAP_HOP_RADIUS = 1;

	// Tweened center for smooth panning
	const viewCenter = tweened({ x: 0, y: 0 }, { duration: 400, easing: cubicOut });

	// Project ALL locations in world-space (stable coordinates)
	let allProjected: ProjectedLocation[] = $derived(
		projectWorld($mapData?.locations ?? [])
	);

	// Filter to minimap-visible locations (1-hop neighbors)
	let nearbyProjected: ProjectedLocation[] = $derived(
		allProjected.filter((l) => l.hops <= MINIMAP_HOP_RADIUS)
	);

	// Find the player's world-space position and update the tweened center
	let playerWorld: { x: number; y: number } | null = $derived.by(() => {
		const p = allProjected.find((l) => $mapData?.player_location === l.id);
		return p ? { x: p.x, y: p.y } : null;
	});

	// Update tweened center when player moves
	$effect(() => {
		if (playerWorld) {
			viewCenter.set({ x: playerWorld.x, y: playerWorld.y });
		}
	});

	// Compute bounding box of nearby locations relative to the player, then derive
	// a viewBox that fits them all with padding.  This auto-zooms the minimap so
	// that neighbours are always visible regardless of geographic spread.
	let viewBox: { x: number; y: number; w: number; h: number } = $derived.by(() => {
		if (nearbyProjected.length === 0) return { x: 0, y: 0, w: W, h: H };

		const cx = $viewCenter.x;
		const cy = $viewCenter.y;

		let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
		for (const l of nearbyProjected) {
			const rx = l.x - cx;
			const ry = l.y - cy;
			if (rx < minX) minX = rx;
			if (rx > maxX) maxX = rx;
			if (ry < minY) minY = ry;
			if (ry > maxY) maxY = ry;
		}

		const PAD = 40; // px padding around the bounding box
		const spanX = maxX - minX + PAD * 2;
		const spanY = maxY - minY + PAD * 2;

		// Maintain the W:H aspect ratio, using whichever axis is tighter
		const aspect = W / H;
		let vbW: number, vbH: number;
		if (spanX / spanY > aspect) {
			vbW = Math.max(spanX, 80);
			vbH = vbW / aspect;
		} else {
			vbH = Math.max(spanY, 60);
			vbW = vbH * aspect;
		}

		// Cap viewBox so labels never scale below readable size (max 2x reference)
		const MAX_SCALE = 2;
		vbW = Math.min(vbW, W * MAX_SCALE);
		vbH = Math.min(vbH, H * MAX_SCALE);

		const midX = (minX + maxX) / 2;
		const midY = (minY + maxY) / 2;

		return { x: midX - vbW / 2, y: midY - vbH / 2, w: vbW, h: vbH };
	});

	// Scale factor: how much bigger the viewBox is compared to the reference W×H.
	// All visual sizes (radii, fonts, strokes) are multiplied by this so they
	// appear the same on screen regardless of geographic spread.
	let s: number = $derived(viewBox.w / W);
	let nodeR: number = $derived(BASE_NODE_R * s);
	let playerR: number = $derived(BASE_PLAYER_R * s);
	// Shrink font when many locations are visible to avoid label pile-up
	let fontSize: number = $derived(
		Math.max(10, BASE_FONT_SIZE - nearbyProjected.length * 2) * s
	);

	// Transform nearby locations to viewBox-local coordinates (centered on player)
	let localProjected: ProjectedLocation[] = $derived(
		nearbyProjected.map((l) => ({
			...l,
			x: l.x - $viewCenter.x - viewBox.x,
			y: l.y - $viewCenter.y - viewBox.y
		}))
	);


	// O(1) location lookup map — avoids O(n) .find() per edge
	let locationMap: Map<string, ProjectedLocation> = $derived(
		new Map(localProjected.map((l) => [l.id, l]))
	);

	let edgeLines: EdgeLine[] = $derived(
		visibleEdges.map(([src, dst]) => {
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
				nodeR: isPlayer(loc) ? playerR : nodeR,
				textW: estimateTextWidth(loc.name, 30, fontSize),
				textH: fontSize
			})),
			viewBox.w,
			viewBox.h,
			edgeLines
		)
	);

	// Edges between 1-hop locations
	let visibleEdges: [string, string][] = $derived.by(() => {
		const nearbyIds = new Set(nearbyProjected.map((l) => l.id));
		return ($mapData?.edges ?? []).filter(([a, b]) => nearbyIds.has(a) && nearbyIds.has(b));
	});

	// Count of off-map connections per visible node (for "road continues" stubs)
	let offMapCounts: Map<string, number> = $derived.by(() => {
		const nearbyIds = new Set(nearbyProjected.map((l) => l.id));
		const counts = new Map<string, number>();
		for (const [a, b] of $mapData?.edges ?? []) {
			if (nearbyIds.has(a) && !nearbyIds.has(b)) counts.set(a, (counts.get(a) ?? 0) + 1);
			if (nearbyIds.has(b) && !nearbyIds.has(a)) counts.set(b, (counts.get(b) ?? 0) + 1);
		}
		return counts;
	});

	let tooltip: string | null = $state(null);

	function isPlayer(loc: MapLocation): boolean {
		return $mapData?.player_location === loc.id;
	}

	async function handleClick(loc: MapLocation) {
		if (!loc.adjacent) return;
		await submitInput(`go to ${loc.name}`);
	}

	function openFullMap() {
		fullMapOpen.set(true);
	}
</script>

<div class="map-panel" data-testid="map-panel">
	<div class="map-header">
		<span class="map-title">Map</span>
		<button class="expand-btn" onclick={openFullMap} title="Open full map (M)">
			<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
				<path d="M1 1h5v2H3v3H1V1zm9 0h5v5h-2V3h-3V1zM1 10h2v3h3v2H1v-5zm12 3h-3v2h5v-5h-2v3z" />
			</svg>
		</button>
	</div>
	{#if $mapData}
		<svg viewBox="0 0 {viewBox.w} {viewBox.h}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Parish minimap">
			<!-- Continuation stubs: short faded lines from nodes with off-map connections -->
			{#each localProjected as loc}
				{@const count = offMapCounts.get(loc.id) ?? 0}
				{@const r = isPlayer(loc) ? playerR : nodeR}
				{#if count > 0 && !isPlayer(loc)}
					{@const cx = viewBox.w / 2}
					{@const cy = viewBox.h / 2}
					{@const angle = Math.atan2(loc.y - cy, loc.x - cx)}
					<line
						x1={loc.x + Math.cos(angle) * (r + 2 * s)}
						y1={loc.y + Math.sin(angle) * (r + 2 * s)}
						x2={loc.x + Math.cos(angle) * (r + 14 * s)}
						y2={loc.y + Math.sin(angle) * (r + 14 * s)}
						class="continuation-stub"
						stroke-width={1 * s}
					/>
				{/if}
			{/each}

			<!-- Edges (reuse pre-computed edgeLines for O(1) rendering) -->
			{#each edgeLines as edge}
				<line x1={edge.x1} y1={edge.y1} x2={edge.x2} y2={edge.y2} class="edge" stroke-width={1 * s} />
			{/each}

			<!-- Leader lines (drawn behind labels) -->
			{#each localProjected as loc, i}
				{@const label = labels[i]}
				{@const r = isPlayer(loc) ? playerR : nodeR}
				{@const threshold = (r + 6 * s) ** 2}
				{#if label && distSq(label.cx, label.cy, loc.x, loc.y) > threshold}
					{@const angle = Math.atan2(label.cy - loc.y, label.cx - loc.x)}
					<line
						x1={loc.x + Math.cos(angle) * (r + 1 * s)}
						y1={loc.y + Math.sin(angle) * (r + 1 * s)}
						x2={label.cx - Math.cos(angle) * Math.min(label.w / 2, 8 * s)}
						y2={label.cy - Math.sin(angle) * Math.min(label.h / 2, 6 * s)}
						class="leader"
						stroke-width={0.3 * s}
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
					<circle cx={loc.x} cy={loc.y} r={isPlayer(loc) ? playerR : nodeR} class="node-circle" stroke-width={1.5 * s} />
					{#if label}
						<text x={label.cx} y={label.cy + fontSize / 2 - 1 * s} class="node-label" font-size={fontSize}>
							{loc.name}
						</text>
					{/if}
				</g>
			{/each}

			<!-- Off-screen indicators removed: confusing at tight zoom -->
		</svg>
		{#if tooltip}
			<div class="tooltip">{tooltip}</div>
		{/if}
	{:else}
		<div class="empty">Loading map&hellip;</div>
	{/if}
</div>

<style>
	.map-panel {
		background: var(--color-panel-bg);
		border-left: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
		padding: 0.5rem;
		position: relative;
		flex-shrink: 0;
	}

	.map-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.25rem;
	}

	.map-title {
		font-size: 0.75rem;
		color: var(--color-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.expand-btn {
		background: none;
		border: 1px solid var(--color-border);
		color: var(--color-muted);
		cursor: pointer;
		padding: 4px;
		line-height: 1;
		border-radius: 3px;
	}

	.expand-btn:hover {
		color: var(--color-accent);
		border-color: var(--color-accent);
		background: var(--color-input-bg);
	}

	svg {
		width: 100%;
		height: auto;
		display: block;
	}

	.edge {
		stroke: var(--color-border);
	}

	.continuation-stub {
		stroke: var(--color-muted);
		opacity: 0.4;
		stroke-dasharray: 3 2;
	}

	.leader {
		stroke: var(--color-muted);
		stroke-dasharray: 1.5 1;
	}

	.node-circle {
		fill: var(--color-panel-bg);
		stroke: var(--color-muted);
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
		fill: var(--color-muted);
		text-anchor: middle;
		pointer-events: none;
	}

	.node.player .node-label {
		fill: var(--color-fg);
	}


	.tooltip {
		position: absolute;
		bottom: 0.5rem;
		right: 0.5rem;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.2rem 0.5rem;
		font-size: 0.8rem;
		border-radius: 3px;
		pointer-events: none;
	}

	.empty {
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.85rem;
		text-align: center;
		padding: 2rem;
	}
</style>
