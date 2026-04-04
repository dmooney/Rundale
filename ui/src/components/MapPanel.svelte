<script lang="ts">
	import { mapData, worldState } from '../stores/game';
	import { fullMapOpen } from '../stores/game';
	import { travelState, getTravelPosition } from '../stores/travel';
	import { submitInput } from '$lib/ipc';
	import { resolveLabels, distSq, estimateTextWidth, type EdgeLine } from '$lib/map-labels';
	import { projectWorld, SCALE, REF_CENTER_LAT, REF_CENTER_LON } from '$lib/map-projection';
	import { getLocationIcon, ICON_PATHS, type LocationIcon } from '$lib/map-icons';
	import type { MapLocation } from '$lib/types';
	import type { ProjectedLocation } from '$lib/map-projection';
	import type { ResolvedLabel } from '$lib/map-labels';
	import { tweened } from 'svelte/motion';
	import { cubicOut } from 'svelte/easing';
	import { onMount } from 'svelte';

	/** All unique icon keys used by current locations, for <defs>. */
	let usedIcons: LocationIcon[] = $derived(
		[...new Set(($mapData?.locations ?? []).map((l) => getLocationIcon(l.name)))]
	);

	/** Reference dimensions — visual sizes are authored relative to this. */
	const W = 320;
	const H = 240;
	/** Base sizes at the reference scale (W × H viewBox). */
	const BASE_NODE_R = 10.5;
	const BASE_PLAYER_R = 15.75;
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


	let edgeLines: EdgeLine[] = $derived(
		visibleEdges.map(([src, dst]) => {
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

	// ── Time-of-day atmosphere ───────────────────────────────────────────
	/** 0 = full daylight, 1 = deep night */
	let nightFactor: number = $derived.by(() => {
		const h = $worldState?.hour ?? 12;
		if (h >= 7 && h <= 17) return 0;       // day
		if (h >= 21 || h <= 4) return 1;        // deep night
		if (h >= 18 && h <= 20) return (h - 17) / 3; // dusk→night
		return (7 - h) / 3;                    // dawn→day
	});

	/** Locations with lights that glow at night. */
	const LIT_PATTERNS = /pub|church|house|village|town|shop|school|letter/i;
	function isLit(name: string): boolean {
		return LIT_PATTERNS.test(name);
	}

	/** Weather-based tint class. */
	let weatherTint: string = $derived.by(() => {
		const w = ($worldState?.weather ?? '').toLowerCase();
		if (w.includes('rain') || w.includes('storm')) return 'weather-rain';
		if (w.includes('fog')) return 'weather-fog';
		return '';
	});

	// ── Travel animation ────────────────────────────────────────────────
	let animFrame = $state(0);

	/** Project a lat/lon to viewBox-local coords (same projection as locations). */
	function projectToLocal(lat: number, lon: number): { x: number; y: number } {
		const cosLat = Math.cos(REF_CENTER_LAT * (Math.PI / 180));
		const wx = (lon - REF_CENTER_LON) * SCALE * cosLat;
		const wy = (REF_CENTER_LAT - lat) * SCALE;
		return {
			x: wx - $viewCenter.x - viewBox.x,
			y: wy - $viewCenter.y - viewBox.y
		};
	}

	let travelDot: { x: number; y: number; progress: number } | null = $derived.by(() => {
		const ts = $travelState;
		if (!ts) return null;
		const pos = getTravelPosition(ts, animFrame);
		if (!pos) return null;
		return { ...projectToLocal(pos.lat, pos.lon), progress: pos.progress };
	});

	/** Edges in the travel path, for highlighting. */
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
		// Only run the animation loop when traveling
		const unsub = travelState.subscribe((ts) => {
			if (ts) {
				raf = requestAnimationFrame(tick);
			} else {
				cancelAnimationFrame(raf);
			}
		});
		return () => {
			cancelAnimationFrame(raf);
			unsub();
		};
	});

	// ── Footprints ─────────────────────────────────────────────────────
	/** Max traversal count for normalizing line thickness. */
	let maxTraversal: number = $derived(
		Math.max(1, ...($mapData?.edge_traversals ?? []).map(([, , c]) => c))
	);

	/** Look up traversal count for an edge (canonical order). */
	function edgeTraversalCount(src: string, dst: string): number {
		const traversals = $mapData?.edge_traversals;
		if (!traversals) return 0;
		for (const [a, b, count] of traversals) {
			if ((a === src && b === dst) || (a === dst && b === src)) return count;
		}
		return 0;
	}

	interface TooltipInfo {
		name: string;
		indoor?: boolean;
		travel_minutes?: number;
		visited?: boolean;
	}

	let tooltip: TooltipInfo | null = $state(null);

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
		<svg viewBox="0 0 {viewBox.w} {viewBox.h}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Parish minimap" class={weatherTint}>
			<defs>
				{#each usedIcons as icon}
					<symbol id="minimap-icon-{icon}" viewBox="0 0 256 256">
						<path d={ICON_PATHS[icon]} />
					</symbol>
				{/each}
				<!-- Night glow filter for lit locations -->
				{#if nightFactor > 0}
					<filter id="minimap-glow" x="-50%" y="-50%" width="200%" height="200%">
						<feGaussianBlur in="SourceGraphic" stdDeviation={4 * s * nightFactor} result="blur" />
						<feColorMatrix in="blur" type="matrix"
							values="1 0 0 0 0.3  0 1 0 0 0.25  0 0 1 0 0.1  0 0 0 0.7 0"
							result="glow" />
						<feMerge>
							<feMergeNode in="glow" />
							<feMergeNode in="SourceGraphic" />
						</feMerge>
					</filter>
				{/if}
			</defs>
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

			<!-- Night overlay — darkens the map at night -->
			{#if nightFactor > 0}
				<rect x="0" y="0" width={viewBox.w} height={viewBox.h}
					fill="black" opacity={nightFactor * 0.45}
					pointer-events="none" />
			{/if}

			<!-- Edges (with footprint thickness) -->
			{#each visibleEdges as [src, dst]}
				{@const a = localProjected.find((p) => p.id === src)}
				{@const b = localProjected.find((p) => p.id === dst)}
				{@const srcLoc = ($mapData?.locations ?? []).find((l) => l.id === src)}
				{@const dstLoc = ($mapData?.locations ?? []).find((l) => l.id === dst)}
				{@const isFrontierEdge = srcLoc?.visited === false || dstLoc?.visited === false}
				{@const traversals = edgeTraversalCount(src, dst)}
				{@const footprintWidth = traversals > 0 ? 1 + 2 * (traversals / maxTraversal) : 1}
				{@const traveling = isTravelEdge(src, dst)}
				{#if a && b}
					<line x1={a.x} y1={a.y} x2={b.x} y2={b.y}
						class="edge"
						class:frontier-edge={isFrontierEdge}
						class:travel-edge={traveling}
						class:footprint={traversals > 0}
						stroke-width={footprintWidth * s}
					/>
				{/if}
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
				{@const r = isPlayer(loc) ? playerR : nodeR}
				{@const icon = getLocationIcon(loc.name)}
				{@const iconSize = r * 2}
				{@const lit = nightFactor > 0 && isLit(loc.name) && loc.visited !== false}
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<g
					class="node"
					class:player={isPlayer(loc)}
					class:adjacent={loc.adjacent}
					class:frontier={loc.visited === false}
					class:lit-node={lit}
					filter={lit ? 'url(#minimap-glow)' : undefined}
					onclick={() => handleClick(loc)}
					onmouseenter={() => (tooltip = { name: loc.name, indoor: loc.indoor, travel_minutes: loc.travel_minutes, visited: loc.visited })}
					onmouseleave={() => (tooltip = null)}
				>
					{#if isPlayer(loc)}
						<circle cx={loc.x} cy={loc.y} r={r} class="node-bg" stroke-width={1.5 * s} />
					{/if}
					<use
						href="#minimap-icon-{icon}"
						x={loc.x - iconSize / 2}
						y={loc.y - iconSize / 2}
						width={iconSize}
						height={iconSize}
						class="node-icon"
					/>
					{#if label}
						<text x={label.cx} y={label.cy + fontSize / 2 - 1 * s} class="node-label" font-size={fontSize}>
							{loc.name}
						</text>
					{/if}
				</g>
			{/each}

			<!-- Travel animation dot -->
			{#if travelDot}
				<circle
					cx={travelDot.x}
					cy={travelDot.y}
					r={nodeR * 0.8}
					class="travel-dot"
					stroke-width={1 * s}
				/>
			{/if}
		</svg>
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
		font-family: var(--font-display);
		font-size: 0.62rem;
		color: var(--color-muted);
		text-transform: uppercase;
		letter-spacing: 0.13em;
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

	.edge.frontier-edge {
		stroke-dasharray: 4 3;
		opacity: 0.4;
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
	}

	.node-label {
		fill: var(--color-muted);
		text-anchor: middle;
		pointer-events: none;
	}

	.node.player .node-label {
		fill: var(--color-fg);
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
		animation: travel-pulse 0.6s ease-in-out infinite alternate;
	}

	@keyframes travel-pulse {
		from { opacity: 0.8; }
		to { opacity: 1; }
	}

	/* ── Night atmosphere ── */
	.node.lit-node .node-icon {
		fill: var(--color-accent);
	}

	.node.lit-node .node-label {
		fill: var(--color-accent);
	}

	/* ── Weather tinting ── */
	svg.weather-rain {
		filter: saturate(0.85) brightness(0.92);
	}

	svg.weather-fog {
		filter: saturate(0.6) contrast(0.85) brightness(1.05);
	}

	.tooltip-unexplored {
		font-style: italic;
	}

	.tooltip {
		position: absolute;
		bottom: 0.5rem;
		right: 0.5rem;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.25rem 0.5rem;
		font-size: 0.75rem;
		border-radius: 3px;
		pointer-events: none;
		line-height: 1.3;
	}

	.tooltip-name {
		font-weight: 600;
	}

	.tooltip-detail {
		color: var(--color-muted);
		font-size: 0.65rem;
	}

	.empty {
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.85rem;
		text-align: center;
		padding: 2rem;
	}
</style>
