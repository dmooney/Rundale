<script lang="ts">
	import { travelDimming } from '../../stores/scene';

	interface Props {
		plate_url: string;
	}

	let { plate_url }: Props = $props();

	// prevUrl starts empty; the first assignment happens inside the effect
	// so Svelte doesn't warn about capturing the initial prop value locally.
	let prevUrl = $state('');
	let transitioning = $state(false);

	$effect(() => {
		// Read the reactive prop inside the effect so Svelte tracks it.
		const next = plate_url;
		if (next !== prevUrl) {
			if (prevUrl !== '') {
				// Only animate after the first real URL arrives.
				transitioning = true;
				const timer = setTimeout(() => {
					prevUrl = next;
					transitioning = false;
				}, 300);
				return () => clearTimeout(timer);
			} else {
				prevUrl = next;
			}
		}
	});
</script>

<div class="plate-wrap" class:dimmed={$travelDimming}>
	<!-- outgoing plate fades out during a cross-fade -->
	{#if transitioning && prevUrl}
		<img
			class="plate outgoing"
			src={prevUrl}
			alt="Departing scene"
			aria-hidden="true"
		/>
	{/if}
	<img
		class="plate"
		class:fading-in={transitioning}
		src={plate_url}
		alt="Scene background"
	/>
</div>

<style>
	.plate-wrap {
		position: absolute;
		inset: 0;
		transition: opacity 0.4s ease;
	}

	.plate-wrap.dimmed {
		opacity: 0.45;
	}

	.plate {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		object-fit: fill;
		image-rendering: pixelated;
		display: block;
		transition: opacity 0.3s ease;
	}

	.plate.outgoing {
		z-index: 1;
		opacity: 0;
		pointer-events: none;
	}

	.plate.fading-in {
		opacity: 0;
		animation: fade-in 0.3s ease forwards;
	}

	@keyframes fade-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}
</style>
