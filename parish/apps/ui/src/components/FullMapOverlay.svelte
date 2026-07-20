<script lang="ts">
	import { onMount } from 'svelte';
	import {
		mapData,
		pushErrorLog,
		formatIpcError,
		uiConfig,
	} from '../stores/game';
	import { travelState } from '../stores/travel';
	import { tiles, currentTileSource } from '../stores/tiles';
	import { submitInput } from '$lib/ipc';
	import { subscribeTileSource } from '$lib/map/tileSync';
	import { MapController, type LocationHoverInfo } from '$lib/map/controller';
	import MapTooltip from './MapTooltip.svelte';
	import type { MapTooltipInfo } from '$lib/types';

	interface Props {
		onclose: () => void;
	}

	let { onclose }: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let controller: MapController | null = null;
	let mounted = $state(false);

	let tooltip: MapTooltipInfo | null = $state(null);

	onMount(() => {
		if (!container) return;
		controller = new MapController({
			container,
			variant: 'full',
			interactive: true,
			tileSource: currentTileSource($tiles),
		});

		controller.onLocationClick(async (info) => {
			if (!info.adjacent) return;
			try {
				await submitInput(`go to ${info.name}`);
			} catch (err) {
				pushErrorLog(
					`Could not travel to ${info.name}: ${formatIpcError(err)}`,
				);
			}
		});

		controller.onLocationHover(
			(info: LocationHoverInfo) => {
				tooltip = {
					name: info.name,
					indoor: info.indoor,
					travel_minutes: info.travelMinutes,
					visited: info.visited,
				};
			},
			() => {
				tooltip = null;
			},
		);

		// Fit to the bounding box of every location on mount so the whole
		// parish is visible at once.
		const m = $mapData;
		if (m && m.locations.length > 0) {
			controller.fitBounds(
				m.locations.map((l) => ({ lat: l.lat, lon: l.lon })),
				60,
			);
			hasFitOnce = true;
		}

		// Keep base tiles in sync with `/tiles` selection.
		const unsubscribeTiles = subscribeTileSource(() => controller);

		mounted = true;

		return () => {
			unsubscribeTiles();
			controller?.destroy();
			controller = null;
		};
	});

	// Tracks whether we've fit the camera to the parish bounds at least
	// once. The initial fitBounds call inside onMount runs synchronously
	// against whatever $mapData was at that moment — if the overlay is
	// opened before mapData has populated (fast 'M' keypress before the
	// initial fetch resolves, or after `/new` while world state is
	// rebuilding), the map stays on MapController's hard-coded default
	// center until the user pans manually. (#351)
	let hasFitOnce = $state(false);

	// Push map data changes into the controller. The first time mapData
	// becomes non-empty after mount, also fit bounds so a delayed first
	// load doesn't leave the user staring at the default Kiltoom view.
	$effect(() => {
		if (!mounted || !controller) return;
		const m = $mapData;
		if (m) {
			controller.updateMap(m);
			if (!hasFitOnce && m.locations.length > 0) {
				controller.fitBounds(
					m.locations.map((l) => ({ lat: l.lat, lon: l.lon })),
					60,
				);
				hasFitOnce = true;
			}
		}
	});

	// Drive travel animation from the shared travel store.
	$effect(() => {
		if (!mounted || !controller) return;
		const ts = $travelState;
		if (ts) {
			controller.startTravel(ts.waypoints, ts.animationMs);
		} else {
			controller.stopTravel();
		}
	});

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape' || e.key === 'm' || e.key === 'M') {
			e.preventDefault();
			e.stopPropagation();
			onclose();
		}
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="map-embed" data-testid="full-map">
	<button
		type="button"
		class="close-btn"
		aria-label="Close full map"
		title="Close (M or Esc)"
		onclick={onclose}
	>
		<span aria-hidden="true">&times;</span>
	</button>
	<div class="map-container" bind:this={container}></div>
	{#if $uiConfig.map_overlay === 'grid'}
		<div class="blueprint-grid-overlay"></div>
	{/if}
	<p class="map-help" aria-label="Map controls">
		Drag to explore · scroll or pinch to zoom · click an outlined place to
		travel
	</p>
	<div class="map-legend" data-testid="map-legend" aria-label="Map legend">
		<span class="legend-item"
			><span class="swatch swatch-player"></span>You are here</span
		>
		<span class="legend-item"
			><span class="swatch swatch-adjacent"></span>Walkable now (click to
			travel)</span
		>
		<span class="legend-item"
			><span class="swatch swatch-visited"></span>Visited</span
		>
		<span class="legend-item"
			><span class="swatch swatch-frontier"></span>Unexplored</span
		>
	</div>
	<MapTooltip info={tooltip} variant="full" />
</div>

<style>
	.map-embed {
		position: absolute;
		inset: 0;
		z-index: 50;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--color-panel-bg);
	}

	.close-btn {
		position: absolute;
		top: 0.5rem;
		right: 0.5rem;
		z-index: 2;
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		color: var(--color-muted);
		font-size: 1.4rem;
		line-height: 1;
		padding: 2px 8px 4px;
		cursor: pointer;
	}

	.close-btn:hover,
	.close-btn:focus-visible {
		color: var(--color-fg);
	}

	.close-btn:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 2px;
	}

	.map-container {
		flex: 1;
		min-height: 0;
		width: 100%;
	}

	.map-legend {
		position: absolute;
		left: 0.75rem;
		bottom: 0.75rem;
		z-index: 2;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		padding: 0.5rem 0.7rem;
		background: color-mix(in srgb, var(--color-panel-bg) 88%, transparent);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		font-family: var(--font-display);
		font-size: 0.6rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-muted);
		pointer-events: none;
	}

	.map-help {
		position: absolute;
		top: 0.75rem;
		left: 50%;
		z-index: 2;
		max-width: calc(100% - 8rem);
		margin: 0;
		padding: 0.35rem 0.65rem;
		transform: translateX(-50%);
		color: var(--color-muted);
		background: color-mix(in srgb, var(--color-panel-bg) 88%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-border) 70%, transparent);
		border-radius: 3px;
		font-family: var(--font-display);
		font-size: 0.64rem;
		letter-spacing: 0.04em;
		text-align: center;
		pointer-events: none;
	}

	.legend-item {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
	}

	.swatch {
		width: 0.65rem;
		height: 0.65rem;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.swatch-player {
		background: var(--color-accent);
		border: 2px solid var(--color-fg);
	}

	.swatch-adjacent {
		background: transparent;
		border: 2px solid var(--color-accent);
	}

	.swatch-visited {
		background: var(--color-muted);
		border: 2px solid transparent;
	}

	.swatch-frontier {
		background: transparent;
		border: 2px dashed var(--color-muted);
		opacity: 0.6;
	}

	@media (max-width: 760px) {
		.map-help {
			top: 0.55rem;
			left: 0.75rem;
			max-width: calc(100% - 5.75rem);
			transform: none;
			font-size: 0.58rem;
			text-align: left;
		}
	}

	/* .travel-dot-marker and @keyframes travel-pulse are defined once in app.css */
</style>
