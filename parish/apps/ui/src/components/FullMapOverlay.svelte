<script lang="ts">
	import { onMount } from 'svelte';
	import { mapData, pushErrorLog, formatIpcError, uiConfig } from '../stores/game';
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

		// Fit to the bounding box of every location on mount so the whole
		// parish is visible at once.
		const m = $mapData;
		if (m && m.locations.length > 0) {
			controller.fitBounds(
				m.locations.map((l) => ({ lat: l.lat, lon: l.lon })),
				60
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
					60
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

	/* .travel-dot-marker and @keyframes travel-pulse are defined once in app.css */
</style>
