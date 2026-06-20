<script lang="ts">
	let {
		variant,
		weatherOverlay = null,
		indoor = false,
	}: {
		variant: string;
		weatherOverlay?: string | null;
		indoor?: boolean;
	} = $props();

	const overlayClass = $derived(
		`scene-overlay variant-${variant.toLowerCase()}${weatherOverlay && !indoor ? ' has-weather' : ''}`,
	);
</script>

<div class={overlayClass} aria-hidden="true"></div>

<style>
	.scene-overlay {
		position: absolute;
		inset: 0;
		pointer-events: none;
		mix-blend-mode: multiply;
		opacity: 0.16;
		transition: opacity 0.2s ease, background 0.2s ease;
	}

	.scene-overlay.variant-night {
		background: #20314e;
		opacity: 0.28;
	}

	.scene-overlay.variant-day {
		background: transparent;
		opacity: 0;
	}

	.scene-overlay.has-weather {
		background:
			linear-gradient(
				115deg,
				rgba(179, 198, 210, 0.12),
				rgba(60, 76, 86, 0.22)
			);
		opacity: 0.22;
	}
</style>
