<!--
  BogLights — faint wandering orbs of pale green-blue light near the bog.
  The tine sídhe, the fairy fires that lure travelers off the safe path.
  2-3 lights drifting in slow sinusoidal paths, fading in and out.
-->
<script lang="ts">
	import type { ActiveEffect } from '$lib/effects';

	interface Props {
		effect: ActiveEffect;
	}
	let { effect }: Props = $props();

	// Generate 2-3 wisps with random positions and timing
	const count = 2 + Math.floor(Math.random() * 2);
	const wisps = Array.from({ length: count }, (_, i) => ({
		x: 10 + Math.random() * 80,
		y: 30 + Math.random() * 50,
		size: 3 + Math.random() * 3,
		driftX: 8 + Math.random() * 12,
		driftY: 5 + Math.random() * 8,
		duration: 6 + Math.random() * 6,
		fadeDelay: i * 2 + Math.random() * 3,
		fadeDuration: 4 + Math.random() * 4,
		hue: 160 + Math.random() * 40, // green-blue range (160-200)
	}));
</script>

<div class="bog-lights">
	{#each wisps as wisp, i}
		<div
			class="wisp"
			style:left="{wisp.x}%"
			style:top="{wisp.y}%"
			style:width="{wisp.size}px"
			style:height="{wisp.size}px"
			style:--drift-x="{wisp.driftX}px"
			style:--drift-y="{wisp.driftY}px"
			style:--wander-dur="{wisp.duration}s"
			style:--fade-delay="{wisp.fadeDelay}s"
			style:--fade-dur="{wisp.fadeDuration}s"
			style:--hue="{wisp.hue}"
		></div>
	{/each}
</div>

<style>
	.bog-lights {
		position: fixed;
		inset: 0;
		pointer-events: none;
	}

	.wisp {
		position: absolute;
		border-radius: 50%;
		background: radial-gradient(
			circle,
			hsla(var(--hue), 80%, 75%, 0.7) 0%,
			hsla(var(--hue), 70%, 60%, 0.3) 40%,
			transparent 70%
		);
		box-shadow:
			0 0 8px 2px hsla(var(--hue), 80%, 65%, 0.4),
			0 0 20px 6px hsla(var(--hue), 60%, 55%, 0.15);
		animation:
			wisp-wander var(--wander-dur) ease-in-out infinite alternate,
			wisp-fade var(--fade-dur) ease-in-out var(--fade-delay) infinite alternate;
		opacity: 0;
	}

	@keyframes wisp-wander {
		0% {
			transform: translate(0, 0);
		}
		25% {
			transform: translate(var(--drift-x), calc(var(--drift-y) * -1));
		}
		50% {
			transform: translate(calc(var(--drift-x) * -0.5), var(--drift-y));
		}
		75% {
			transform: translate(calc(var(--drift-x) * -1), calc(var(--drift-y) * -0.5));
		}
		100% {
			transform: translate(calc(var(--drift-x) * 0.3), calc(var(--drift-y) * 0.7));
		}
	}

	@keyframes wisp-fade {
		0%, 100% { opacity: 0; }
		20%      { opacity: 0.6; }
		80%      { opacity: 0.5; }
	}
</style>
