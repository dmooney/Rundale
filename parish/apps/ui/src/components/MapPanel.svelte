<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { mapData, fullMapOpen, pushErrorLog, formatIpcError } from '../stores/game';
	import { travelState } from '../stores/travel';
	import { tiles, currentTileSource } from '../stores/tiles';
	import { submitInput } from '$lib/ipc';
	import { MapController, type LocationHoverInfo } from '$lib/map/controller';
	import type { MapLocation } from '$lib/types';

	/** Only show locations within this many hops on the minimap. */
	const MINIMAP_HOP_RADIUS = 1;

	let container: HTMLDivElement | undefined = $state();
	let controller: MapController | null = null;
	let mounted = $state(false);
	/** Current pixel size of the container, updated when MapLibre resizes. */
	let containerSize = $state({ width: 0, height: 0 });
	/** Screen-space continuation stubs, recomputed on every map 'move' event. */
	let stubs = $state<
		Array<{ x1: number; y1: number; x2: number; y2: number }>
	>([]);

	interface TooltipInfo {
		name: string;
		indoor?: boolean;
		travel_minutes?: number;
		visited?: boolean;
	}

	let tooltip: TooltipInfo | null = $state(null);

	/** Computes the set of location IDs visible on the minimap. */
	function visibleIdSet(locations: MapLocation[]): Set<string> {
		return new Set(
			locations.filter((l) => l.hops <= MINIMAP_HOP_RADIUS).map((l) => l.id)
		);
	}

	/**
	 * Given the player and their visible neighbors, returns a symmetric
	 * lat/lon bounding box centered on the player that encloses every
	 * neighbor plus a small halo. Feeding this into `fitBounds` produces a
	 * player-centered view whose zoom scales with neighbor spread.
	 *
	 * The half-span is clamped to `MAX_HALF_SPAN` so one unusually distant
	 * 1-hop neighbor (e.g. a road connection to the next townland) can't
	 * force the camera so far out that the tile base becomes unreadable.
	 * Neighbors beyond the clamp render as dots on the map edge with their
	 * connection line sailing off the canvas.
	 */
	const MAX_HALF_SPAN = 0.008;
	function computePlayerCenteredBounds(
		player: MapLocation,
		neighbors: MapLocation[]
	): Array<{ lat: number; lon: number }> {
		if (neighbors.length === 0) {
			// No neighbors — construct a small fixed box around the player.
			const pad = 0.003; // ~300m
			return [
				{ lat: player.lat - pad, lon: player.lon - pad },
				{ lat: player.lat + pad, lon: player.lon + pad }
			];
		}
		let maxDLat = 0.001;
		let maxDLon = 0.001;
		for (const n of neighbors) {
			maxDLat = Math.max(maxDLat, Math.abs(n.lat - player.lat));
			maxDLon = Math.max(maxDLon, Math.abs(n.lon - player.lon));
		}
		maxDLat = Math.min(maxDLat, MAX_HALF_SPAN);
		maxDLon = Math.min(maxDLon, MAX_HALF_SPAN);
		// Add a small halo so the edge nodes aren't flush against the border.
		maxDLat *= 1.4;
		maxDLon *= 1.4;
		return [
			{ lat: player.lat - maxDLat, lon: player.lon - maxDLon },
			{ lat: player.lat + maxDLat, lon: player.lon + maxDLon }
		];
	}

	/** Recomputes continuation stub positions from the current map state. */
	function recomputeStubs() {
		if (!controller) {
			stubs = [];
			return;
		}
		const m = $mapData;
		if (!m) {
			stubs = [];
			return;
		}
		const visible = visibleIdSet(m.locations);
		const next: Array<{ x1: number; y1: number; x2: number; y2: number }> = [];
		const size = controller.getContainerSize();
		containerSize = size;
		const cx = size.width / 2;
		const cy = size.height / 2;

		// Count off-map edges per visible node.
		const offMap = new Map<string, number>();
		for (const [a, b] of m.edges) {
			if (visible.has(a) && !visible.has(b))
				offMap.set(a, (offMap.get(a) ?? 0) + 1);
			if (visible.has(b) && !visible.has(a))
				offMap.set(b, (offMap.get(b) ?? 0) + 1);
		}

		const STUB_INNER = 8;
		const STUB_OUTER = 22;

		for (const loc of m.locations) {
			if (!visible.has(loc.id)) continue;
			if (loc.id === m.player_location) continue;
			const count = offMap.get(loc.id) ?? 0;
			if (count === 0) continue;

			const { x, y } = controller.projectToScreen(loc.lat, loc.lon);
			const angle = Math.atan2(y - cy, x - cx);
			next.push({
				x1: x + Math.cos(angle) * STUB_INNER,
				y1: y + Math.sin(angle) * STUB_INNER,
				x2: x + Math.cos(angle) * STUB_OUTER,
				y2: y + Math.sin(angle) * STUB_OUTER
			});
		}

		stubs = next;
	}

	onMount(() => {
		if (!container) return;
		controller = new MapController({
			container,
			variant: 'minimap',
			interactive: false,
			tileSource: currentTileSource($tiles)
		});

		controller.onLocationClick(async (info) => {
			if (!info.adjacent) return;
			try {
				await submitInput(`go to ${info.name}`);
			} catch (err) {
				pushErrorLog(
					`Could not travel to ${info.name}: ${formatIpcError(err)}`
				);
			}
		});

		controller.onLocationHover(
			(info: LocationHoverInfo) => {
				tooltip = {
					name: info.name,
					indoor: info.indoor,
					travel_minutes: info.travelMinutes,
					visited: info.visited
				};
			},
			() => {
				tooltip = null;
			}
		);

		// Recompute stubs once on mount and then only when the camera
		// actually moves or the container resizes (#350). The previous
		// requestAnimationFrame loop ran every frame for the lifetime
		// of the component — recomputing identical stub geometry over
		// and over and burning CPU/GPU even when the user wasn't
		// interacting. The controller now exposes `addMoveListener`
		// so we can subscribe surgically.
		recomputeStubs();
		const unsubscribeMove = controller.addMoveListener(recomputeStubs);

		mounted = true;

		return () => {
			unsubscribeMove();
			controller?.destroy();
			controller = null;
		};
	});

	// Push map data changes into the controller and reframe the camera.
	//
	// `$mapData` updates every game tick (NPC movement, clock, etc.), so we
	// skip the instantaneous `fitBounds` call while travel is animating —
	// otherwise the minimap re-centers on the player's origin between ticks,
	// pinning the travel dot near the viewport center even though it's
	// sliding correctly in world coordinates.
	$effect(() => {
		if (!mounted || !controller) return;
		const m = $mapData;
		if (!m) return;
		const visible = visibleIdSet(m.locations);
		controller.updateMap(m, visible);

		if ($travelState) return;

		const player = m.locations.find((l) => l.id === m.player_location);
		if (!player) return;
		const neighbors = m.locations.filter(
			(l) => visible.has(l.id) && l.id !== player.id
		);
		controller.fitBounds(
			computePlayerCenteredBounds(player, neighbors),
			16
		);
	});

	// Drive travel animation from the shared travel store.
	//
	// `destinationBounds` reads `$mapData`, but we wrap it in `untrack` so
	// the effect only runs on `$travelState` transitions — without this,
	// every mapData tick during travel restarts the animation.
	$effect(() => {
		if (!mounted || !controller) return;
		const ts = $travelState;
		if (ts) {
			const bounds = untrack(() => destinationBounds(ts.destination));
			controller.startTravel(ts.waypoints, ts.animationMs, bounds);
		} else {
			controller.stopTravel();
		}
	});

	/**
	 * Builds the bounding box we want the minimap to settle into once the
	 * player arrives at `destId`. We use the same player-centered algorithm
	 * the `$mapData` effect uses — just evaluated against the destination's
	 * neighbors (looked up from the current graph) rather than the current
	 * player's. Passing this to `startTravel` lets the camera smoothly
	 * interpolate both center AND zoom over the animation window, so the
	 * post-travel view is already in place when the dot arrives.
	 */
	function destinationBounds(
		destId: string
	): Array<{ lat: number; lon: number }> | undefined {
		const m = $mapData;
		if (!m) return undefined;
		const dest = m.locations.find((l) => l.id === destId);
		if (!dest) return undefined;
		const neighborIds = new Set<string>();
		for (const [a, b] of m.edges) {
			if (a === destId) neighborIds.add(b);
			else if (b === destId) neighborIds.add(a);
		}
		const neighbors = m.locations.filter((l) => neighborIds.has(l.id));
		return computePlayerCenteredBounds(dest, neighbors);
	}

	// Keep minimap base tiles in sync with `/tiles` selection.
	$effect(() => {
		if (!mounted || !controller) return;
		controller.setTileSource(currentTileSource($tiles));
	});

	function toggleFullMap() {
		fullMapOpen.update((v) => !v);
	}
</script>

<div class="map-panel" data-testid="map-panel">
	<div class="map-header">
		<span class="map-title">Map</span>
		<button type="button" class="expand-btn" onclick={toggleFullMap} title="Toggle full map (M)" aria-label="Toggle full map">
			<svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" aria-hidden="true" focusable="false">
				<path d="M1 1h5v2H3v3H1V1zm9 0h5v5h-2V3h-3V1zM1 10h2v3h3v2H1v-5zm12 3h-3v2h5v-5h-2v3z" />
			</svg>
		</button>
	</div>
	<!-- map-wrap is always in the DOM so onMount can bind the container
	     before mapData arrives. MapLibre needs a stable element to attach to. -->
	<div class="map-wrap">
		<div class="map-container" bind:this={container}></div>
		{#if $mapData && stubs.length > 0}
			<svg
				class="stub-overlay"
				viewBox="0 0 {containerSize.width} {containerSize.height}"
				width={containerSize.width}
				height={containerSize.height}
			>
				{#each stubs as stub}
					<line
						x1={stub.x1}
						y1={stub.y1}
						x2={stub.x2}
						y2={stub.y2}
						class="continuation-stub"
					/>
				{/each}
			</svg>
		{/if}
		{#if !$mapData}
			<div class="empty">Loading map&hellip;</div>
		{/if}
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

	.expand-btn:hover,
	.expand-btn:focus-visible {
		color: var(--color-accent);
		border-color: var(--color-accent);
		background: var(--color-input-bg);
	}

	.map-wrap {
		position: relative;
		width: 100%;
		height: 240px;
	}

	.map-container {
		position: absolute;
		inset: 0;
	}

	.stub-overlay {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	.continuation-stub {
		stroke: var(--color-muted);
		stroke-width: 1.2;
		opacity: 0.5;
		stroke-dasharray: 3 2;
	}

	/* Travel animation dot — also defined in FullMapOverlay.svelte; mirrored
	   here because Svelte scopes :global styles to the mounting component,
	   so the minimap needs its own copy when the full map isn't open.
	   The pulse animates opacity + box-shadow only; animating `transform`
	   would clobber the `translate(…)` MapLibre sets each frame to position
	   the marker, collapsing it to the canvas top-left. */
	:global(.travel-dot-marker) {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		background: var(--color-accent);
		border: 2px solid var(--color-fg);
		animation: travel-pulse 0.6s ease-in-out infinite alternate;
		pointer-events: none;
	}

	@keyframes travel-pulse {
		from {
			opacity: 0.85;
			box-shadow: 0 0 4px var(--color-accent);
		}
		to {
			opacity: 1;
			box-shadow: 0 0 12px var(--color-accent);
		}
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
		z-index: 10;
	}

	.tooltip-name {
		font-weight: 600;
	}

	.tooltip-detail {
		color: var(--color-muted);
		font-size: 0.65rem;
	}

	.empty {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-muted);
		font-style: italic;
		font-size: 0.85rem;
		pointer-events: none;
	}
</style>
