<!--
  RainStreaks — diagonal rain lines on the game window.
  Intensity adjusts for LightRain vs HeavyRain/Storm.
  Fades in over 2s on mount.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import type { ActiveEffect } from '$lib/effects';
	import { worldState } from '../../stores/game';

	interface Props {
		effect: ActiveEffect;
	}
	let { effect }: Props = $props();

	let visible = $state(false);

	const heavy = $derived(
		$worldState?.weather === 'HeavyRain' || $worldState?.weather === 'Storm'
	);

	onMount(() => {
		// Trigger fade-in on next frame
		requestAnimationFrame(() => { visible = true; });
	});
</script>

<div
	class="rain"
	class:visible
	class:heavy
></div>

<style>
	.rain {
		position: fixed;
		inset: -20px;
		pointer-events: none;
		opacity: 0;
		transition: opacity 2s ease-in;
	}

	.rain.visible {
		opacity: 1;
	}

	/* Front layer — closer, faster streaks */
	.rain::before {
		content: '';
		position: absolute;
		inset: 0;
		background: repeating-linear-gradient(
			170deg,
			transparent,
			transparent 58px,
			rgba(180, 200, 220, 0.10) 58px,
			rgba(180, 200, 220, 0.10) 59px
		);
		animation: rain-fall-front 3s linear infinite;
	}

	/* Back layer — different angle, slower for depth */
	.rain::after {
		content: '';
		position: absolute;
		inset: 0;
		background: repeating-linear-gradient(
			165deg,
			transparent,
			transparent 80px,
			rgba(170, 190, 210, 0.07) 80px,
			rgba(170, 190, 210, 0.07) 81px
		);
		animation: rain-fall-back 4s linear infinite;
	}

	/* Heavy rain overrides */
	.rain.heavy::before {
		background: repeating-linear-gradient(
			170deg,
			transparent,
			transparent 33px,
			rgba(180, 200, 220, 0.15) 33px,
			rgba(180, 200, 220, 0.15) 34px
		);
		animation-duration: 1.5s;
	}

	.rain.heavy::after {
		background: repeating-linear-gradient(
			165deg,
			transparent,
			transparent 50px,
			rgba(170, 190, 210, 0.10) 50px,
			rgba(170, 190, 210, 0.10) 51px
		);
		animation-duration: 2s;
	}

	@keyframes rain-fall-front {
		from { transform: translate(0, -60px); }
		to   { transform: translate(-20px, 60px); }
	}

	@keyframes rain-fall-back {
		from { transform: translate(0, -80px); }
		to   { transform: translate(-15px, 80px); }
	}
</style>
