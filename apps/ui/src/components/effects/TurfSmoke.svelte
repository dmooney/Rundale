<!--
  TurfSmoke — thin wisps of smoke rising from the bottom of the screen.
  The smell of turf you can almost see. Near cottages and the pub.
-->
<script lang="ts">
	import type { ActiveEffect } from '$lib/effects';

	interface Props {
		effect: ActiveEffect;
	}
	let { effect }: Props = $props();

	// Generate 2-3 smoke wisps at random horizontal positions
	const wisps = Array.from({ length: 2 + Math.floor(Math.random() * 2) }, () => ({
		x: 10 + Math.random() * 80,
		drift: -10 + Math.random() * 20,
		duration: 8 + Math.random() * 6,
		delay: Math.random() * 4,
		width: 20 + Math.random() * 30,
		opacity: 0.04 + Math.random() * 0.04,
	}));
</script>

<div class="turf-smoke">
	{#each wisps as wisp}
		<div
			class="wisp"
			style:left="{wisp.x}%"
			style:--drift="{wisp.drift}px"
			style:--dur="{wisp.duration}s"
			style:--delay="{wisp.delay}s"
			style:--w="{wisp.width}px"
			style:--peak-opacity="{wisp.opacity}"
		></div>
	{/each}
</div>

<style>
	.turf-smoke {
		position: fixed;
		inset: 0;
		pointer-events: none;
	}

	.wisp {
		position: absolute;
		bottom: 0;
		width: var(--w);
		height: 120px;
		background: linear-gradient(
			to top,
			rgba(160, 150, 130, var(--peak-opacity)) 0%,
			rgba(160, 150, 130, calc(var(--peak-opacity) * 0.5)) 40%,
			transparent 100%
		);
		border-radius: 50% 50% 0 0;
		filter: blur(8px);
		animation: smoke-rise var(--dur) ease-out var(--delay) infinite;
		opacity: 0;
	}

	@keyframes smoke-rise {
		0% {
			transform: translateY(0) translateX(0) scaleX(1);
			opacity: 0;
		}
		10% {
			opacity: 1;
		}
		60% {
			opacity: 0.7;
		}
		100% {
			transform: translateY(-200px) translateX(var(--drift)) scaleX(1.8);
			opacity: 0;
		}
	}
</style>
