<script lang="ts">
	import { mapData } from '../stores/game';
	import { travelState, getTravelPosition } from '../stores/travel';
	import { submitInput } from '$lib/ipc';
	import { resolveLabels, distSq, estimateTextWidth, type EdgeLine } from '$lib/map-labels';
	import { projectWorld, SCALE, REF_CENTER_LAT, REF_CENTER_LON } from '$lib/map-projection';
	import { getLocationIcon, ICON_PATHS, type LocationIcon } from '$lib/map-icons';
	import type { MapLocation } from '$lib/types';
	import type { ProjectedLocation } from '$lib/map-projection';
	import type { ResolvedLabel } from '$lib/map-labels';
	import { onMount } from 'svelte';

	/** All unique icon keys used by current locations, for <defs>. */
	let usedIcons: LocationIcon[] = $derived(
		[...new Set(($mapData?.locations ?? []).map((l) => getLocationIcon(l.name)))]
	);

	interface Props {
		onclose: () => void;
	}

	let { onclose }: Props = $props();

	const NODE_R = 8.75;
	const PLAYER_R = 14;
	const LABEL_FONT_SIZE = 11;
	const MIN_ZOOM = 0.5;
	const MAX_ZOOM = 4;

	let zoom = $state(1);
	let panX = $state(0);
	let panY = $state(0);
	let dragging = $state(false);
	let lastPointer = $state({ x: 0, y: 0 });
	interface TooltipInfo {
		name: string;
		indoor?: boolean;
		travel_minutes?: number;
		visited?: boolean;
	}

	let tooltip: TooltipInfo | null = $state(null);

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

	let fullEdgeLines: EdgeLine[] = $derived(
		($mapData?.edges ?? []).map(([src, dst]) => {
			const a = localProjected.find((p) => p.id === src);
			const b = localProjected.find((p) => p.id === dst);
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

	const LIT_PATTERNS = /pub|church|house|village|town|shop|school|letter/i;
	function isLit(name: string): boolean {
		return LIT_PATTERNS.test(name);
	}

	// ── Travel animation ────────────────────────────────────────────────
	let animFrame = $state(0);

	function projectToLocal(lat: number, lon: number): { x: number; y: number } {
		const cosLat = Math.cos(REF_CENTER_LAT * (Math.PI / 180));
		const wx = (lon - REF_CENTER_LON) * SCALE * cosLat;
		const wy = (REF_CENTER_LAT - lat) * SCALE;
		return { x: wx - bounds.minX, y: wy - bounds.minY };
	}

	let travelDot: { x: number; y: number; progress: number } | null = $derived.by(() => {
		const ts = $travelState;
		if (!ts) return null;
		const pos = getTravelPosition(ts, animFrame);
		if (!pos) return null;
		return { ...projectToLocal(pos.lat, pos.lon), progress: pos.progress };
	});

	let travelEdgeIds: Set<string> | null = $derived.by(() => {
		const ts = $travelState;
		if (!ts || ts.waypoints.length < 2) return null;
		const set = new Set<string>();
		for (let i = 0; i < ts.waypoints.length - 1; i++) {
			const a = ts.waypoints[i].id;
			const b = ts.waypoints[i + 1].id;
			set.add(a < b ? `${a}-${b}` : `${b}-${a}`);
		}
		return set;
	});

	function isTravelEdge(src: string, dst: string): boolean {
		if (!travelEdgeIds) return false;
		const key = src < dst ? `${src}-${dst}` : `${dst}-${src}`;
		return travelEdgeIds.has(key);
	}

	onMount(() => {
		let raf: number;
		function tick() {
			animFrame = performance.now();
			raf = requestAnimationFrame(tick);
		}
		const unsub = travelState.subscribe((ts) => {
			if (ts) { raf = requestAnimationFrame(tick); }
			else { cancelAnimationFrame(raf); }
		});
		return () => { cancelAnimationFrame(raf); unsub(); };
	});

	// ── Footprints ─────────────────────────────────────────────────────
	let maxTraversal: number = $derived(
		Math.max(1, ...($mapData?.edge_traversals ?? []).map(([, , c]) => c))
	);

	function edgeTraversalCount(src: string, dst: string): number {
		const traversals = $mapData?.edge_traversals;
		if (!traversals) return 0;
		for (const [a, b, count] of traversals) {
			if ((a === src && b === dst) || (a === dst && b === src)) return count;
		}
		return 0;
	}

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

</script>

<svelte:window onkeydown={handleKeydown} />

<div class="map-embed">
	<div class="overlay-header">
		<span class="overlay-title">Parish Map</span>
		<span class="overlay-hint">Scroll to zoom &middot; Drag to pan &middot; M to close</span>
		<button class="close-btn" onclick={onclose} title="Close (M)">&times;</button>
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
				<defs>
					{#each usedIcons as icon}
						<symbol id="fullmap-icon-{icon}" viewBox="0 0 256 256">
							<path d={ICON_PATHS[icon]} />
						</symbol>
					{/each}
					<filter id="fullmap-glow" x="-50%" y="-50%" width="200%" height="200%">
						<feGaussianBlur in="SourceGraphic" stdDeviation="5" result="blur" />
						<feColorMatrix
							in="blur"
							type="matrix"
							values="1 0 0 0 0.3  0 1 0 0 0.25  0 0 1 0 0.1  0 0 0 0.7 0"
							result="glow"
						/>
						<feMerge>
							<feMergeNode in="glow" />
							<feMergeNode in="SourceGraphic" />
						</feMerge>
					</filter>
				</defs>

				<!-- Edges (with footprints and travel highlight) -->
				{#each $mapData?.edges ?? [] as [src, dst]}
					{@const a = localProjected.find((p) => p.id === src)}
					{@const b = localProjected.find((p) => p.id === dst)}
					{@const srcLoc = ($mapData?.locations ?? []).find((l) => l.id === src)}
					{@const dstLoc = ($mapData?.locations ?? []).find((l) => l.id === dst)}
					{@const isFrontierEdge = srcLoc?.visited === false || dstLoc?.visited === false}
					{@const traversals = edgeTraversalCount(src, dst)}
					{@const footprintWidth = traversals > 0 ? 1.5 + 2 * (traversals / maxTraversal) : 1.5}
					{@const traveling = isTravelEdge(src, dst)}
					{#if a && b}
						<line x1={a.x} y1={a.y} x2={b.x} y2={b.y}
							class="edge"
							class:frontier-edge={isFrontierEdge}
							class:travel-edge={traveling}
							class:footprint={traversals > 0}
							stroke-width={footprintWidth}
						/>
					{/if}
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
					{@const r = isPlayer(loc) ? PLAYER_R : NODE_R}
					{@const icon = getLocationIcon(loc.name)}
					{@const iconSize = r * 2}
					{@const lit = isLit(loc.name) && loc.visited !== false}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<g
						class="node"
						class:player={isPlayer(loc)}
						class:adjacent={loc.adjacent}
						class:frontier={loc.visited === false}
						class:lit-node={lit}
						filter={lit ? 'url(#fullmap-glow)' : undefined}
						onclick={() => handleClick(loc)}
						onmouseenter={() => (tooltip = { name: loc.name, indoor: loc.indoor, travel_minutes: loc.travel_minutes, visited: loc.visited })}
						onmouseleave={() => (tooltip = null)}
					>
						{#if isPlayer(loc)}
							<circle cx={loc.x} cy={loc.y} r={r} class="node-bg" />
						{/if}
						<use
							href="#fullmap-icon-{icon}"
							x={loc.x - iconSize / 2}
							y={loc.y - iconSize / 2}
							width={iconSize}
							height={iconSize}
							class="node-icon"
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

				<!-- Travel animation dot -->
				{#if travelDot}
					<circle
						cx={travelDot.x}
						cy={travelDot.y}
						r={NODE_R * 0.8}
						class="travel-dot"
					/>
				{/if}
			</svg>
		</div>
	{#if tooltip}
		<div class="tooltip">
			<div class="tooltip-name">{tooltip.name}</div>
			{#if tooltip.visited === false}
				<div class="tooltip-detail tooltip-unexplored">Unexplored</div>
			{:else}
				{#if tooltip.indoor !== undefined}
					<div class="tooltip-detail">{tooltip.indoor ? 'Indoor' : 'Outdoor'}</div>
				{/if}
				{#if tooltip.travel_minutes != null && tooltip.travel_minutes > 0}
					<div class="tooltip-detail">{tooltip.travel_minutes} min walk</div>
				{/if}
			{/if}
		</div>
	{/if}
</div>

<style>
	.map-embed {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--color-panel-bg);
		position: relative;
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

	.edge.frontier-edge {
		stroke-dasharray: 4 3;
		opacity: 0.4;
	}

	.leader {
		stroke: var(--color-muted);
		stroke-width: 0.4;
		stroke-dasharray: 2 1;
	}

	.node-icon {
		fill: var(--color-muted);
		cursor: default;
	}

	.node.adjacent .node-icon {
		fill: var(--color-accent);
		cursor: pointer;
	}

	.node.adjacent:hover .node-icon {
		fill: var(--color-fg);
	}

	.node.player .node-icon {
		fill: var(--color-fg);
	}

	.node-bg {
		fill: var(--color-accent);
		stroke: var(--color-fg);
		stroke-width: 1.5;
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

	.node.frontier .node-icon {
		opacity: 0.4;
	}

	.node.frontier .node-label {
		opacity: 0.5;
		font-style: italic;
	}

	.node.frontier.adjacent .node-icon {
		opacity: 0.6;
		cursor: pointer;
	}

	/* ── Footprints (worn paths) ── */
	.edge.footprint {
		opacity: 0.85;
	}

	/* ── Travel animation ── */
	.edge.travel-edge {
		stroke: var(--color-accent);
		opacity: 0.9;
	}

	.travel-dot {
		fill: var(--color-accent);
		stroke: var(--color-fg);
		stroke-width: 1.5;
		animation: travel-pulse 0.6s ease-in-out infinite alternate;
	}

	@keyframes travel-pulse {
		from { opacity: 0.8; }
		to { opacity: 1; }
	}

	.node.lit-node .node-icon {
		fill: var(--color-accent);
	}

	.node.lit-node .node-label {
		fill: var(--color-accent);
	}

	.tooltip-unexplored {
		font-style: italic;
	}

	.tooltip {
		position: absolute;
		bottom: 0.75rem;
		right: 0.75rem;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.3rem 0.6rem;
		font-size: 0.8rem;
		border-radius: 4px;
		pointer-events: none;
		line-height: 1.3;
	}

	.tooltip-name {
		font-weight: 600;
	}

	.tooltip-detail {
		color: var(--color-muted);
		font-size: 0.7rem;
	}
</style>
