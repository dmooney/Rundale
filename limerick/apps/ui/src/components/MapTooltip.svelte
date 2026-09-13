<script lang="ts">
	import type { MapTooltipInfo } from '$lib/types';

	let { info, variant = 'minimap' }: { info: MapTooltipInfo | null; variant?: 'minimap' | 'full' } = $props();
</script>

{#if info}
	<div class="tooltip" class:tooltip--full={variant === 'full'} role="tooltip">
		<div class="tooltip-name">{info.name}</div>
		{#if info.visited === false}
			<div class="tooltip-detail tooltip-unexplored">Unexplored</div>
		{:else}
			{#if info.indoor !== undefined}
				<div class="tooltip-detail">{info.indoor ? 'Indoor' : 'Outdoor'}</div>
			{/if}
			{#if info.travel_minutes != null && info.travel_minutes > 0}
				<div class="tooltip-detail">{info.travel_minutes} min walk</div>
			{/if}
		{/if}
	</div>
{/if}

<style>
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

	.tooltip--full {
		bottom: 0.75rem;
		right: 0.75rem;
		padding: 0.3rem 0.6rem;
		font-size: 0.8rem;
		border-radius: 4px;
	}

	.tooltip--full .tooltip-detail {
		font-size: 0.7rem;
	}

	.tooltip-name {
		font-weight: 600;
	}

	.tooltip-detail {
		color: var(--color-muted);
		font-size: 0.65rem;
	}

	.tooltip-unexplored {
		font-style: italic;
	}
</style>
