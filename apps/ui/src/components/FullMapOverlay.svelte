<script lang="ts">
	import { onMount } from 'svelte';
	import { mapData } from '../stores/game';
	import { travelState } from '../stores/travel';
	import { tiles, currentTileSource } from '../stores/tiles';
	import { submitInput } from '$lib/ipc';
	import { MapController, type LocationHoverInfo } from '$lib/map/controller';

	interface Props {
		onclose: () => void;
	}

	let { onclose }: Props = $props();

	let container: HTMLDivElement | undefined = $state();
	let controller: MapController | null = null;
	let mounted = $state(false);

	interface TooltipInfo {
		name: string;
		indoor?: boolean;
		travel_minutes?: number;
		visited?: boolean;
	}

	let tooltip: TooltipInfo | null = $state(null);

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
			await submitInput(`go to ${info.name}`);
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
		}

		mounted = true;

		return () => {
			controller?.destroy();
			controller = null;
		};
	});

	// Push map data changes into the controller.
	$effect(() => {
		if (!mounted || !controller) return;
		const m = $mapData;
		if (m) controller.updateMap(m);
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

	// Swap the base tile source when the user toggles via `/tiles`.
	$effect(() => {
		if (!mounted || !controller) return;
		controller.setTileSource(currentTileSource($tiles));
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

<div class="map-embed">
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

	/* ── Travel animation dot (HTML marker) ──
	   Animating `transform` would clobber the `translate(…)` MapLibre sets
	   each frame to position the marker, collapsing it to the canvas
	   top-left. Pulse via opacity + box-shadow only. */
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
		z-index: 10;
	}

	.tooltip-name {
		font-weight: 600;
	}

	.tooltip-detail {
		color: var(--color-muted);
		font-size: 0.7rem;
	}

	.tooltip-unexplored {
		font-style: italic;
	}
</style>
