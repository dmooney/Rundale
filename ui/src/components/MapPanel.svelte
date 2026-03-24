<script lang="ts">
	import { mapData } from '../stores/game';
	import { submitInput } from '$lib/ipc';
	import type { MapLocation } from '$lib/types';

	const W = 320;
	const H = 240;

	interface ProjectedLocation extends MapLocation {
		x: number;
		y: number;
	}

	let projected: ProjectedLocation[] = $derived(project($mapData?.locations ?? []));
	let tooltip: string | null = $state(null);

	function project(locs: MapLocation[]): ProjectedLocation[] {
		const hasCoords = locs.some((l) => l.lat !== 0 || l.lon !== 0);
		if (!hasCoords || locs.length === 0) {
			// Grid fallback layout
			return locs.map((l, i) => ({
				...l,
				x: ((i % 5) + 0.5) * (W / 5),
				y: (Math.floor(i / 5) + 0.5) * (H / Math.ceil(locs.length / 5))
			}));
		}

		const lats = locs.map((l) => l.lat);
		const lons = locs.map((l) => l.lon);
		const minLat = Math.min(...lats);
		const maxLat = Math.max(...lats);
		const minLon = Math.min(...lons);
		const maxLon = Math.max(...lons);
		const padX = (W * 0.1) / 2;
		const padY = (H * 0.1) / 2;
		const rangeX = maxLon - minLon || 1;
		const rangeY = maxLat - minLat || 1;

		return locs.map((l) => ({
			...l,
			x: padX + ((l.lon - minLon) / rangeX) * (W - padX * 2),
			y: padY + ((maxLat - l.lat) / rangeY) * (H - padY * 2)
		}));
	}

	function isPlayer(loc: MapLocation): boolean {
		return $mapData?.player_location === loc.id;
	}

	async function handleClick(loc: MapLocation) {
		if (!loc.adjacent) return;
		await submitInput(`go to ${loc.name}`);
	}
</script>

<div class="map-panel">
	<div class="map-title">Parish Map</div>
	{#if $mapData}
		<svg viewBox="0 0 {W} {H}" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="Parish map">
			<!-- Edges -->
			{#each $mapData.edges as [src, dst]}
				{@const a = projected.find((p) => p.id === src)}
				{@const b = projected.find((p) => p.id === dst)}
				{#if a && b}
					<line x1={a.x} y1={a.y} x2={b.x} y2={b.y} class="edge" />
				{/if}
			{/each}

			<!-- Location nodes -->
			{#each projected as loc}
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
					<circle cx={loc.x} cy={loc.y} r={isPlayer(loc) ? 8 : 5} class="node-circle" />
					<text x={loc.x} y={loc.y + (isPlayer(loc) ? 18 : 14)} class="node-label">
						{loc.name.length > 14 ? loc.name.slice(0, 12) + '…' : loc.name}
					</text>
				</g>
			{/each}
		</svg>
		{#if tooltip}
			<div class="tooltip">{tooltip}</div>
		{/if}
	{:else}
		<div class="empty">Loading map…</div>
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

	.map-title {
		font-size: 0.75rem;
		color: var(--color-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		margin-bottom: 0.25rem;
	}

	svg {
		width: 100%;
		height: auto;
		display: block;
	}

	.edge {
		stroke: var(--color-border);
		stroke-width: 1;
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
		font-size: 7px;
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
