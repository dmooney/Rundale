<!--
  DustMotes — tiny particles drifting through a sunbeam indoors.
  A faint diagonal light stripe with 1-2px motes in slow random walks.
  The feeling of a quiet cottage with sun through a small window.
-->
<script lang="ts">
	import type { ActiveEffect } from '$lib/effects';
	import { onMount } from 'svelte';

	interface Props {
		effect: ActiveEffect;
	}
	let { effect }: Props = $props();

	let visible = $state(false);

	// Sunbeam angle and position
	const beamAngle = 25 + Math.random() * 20; // 25-45 degrees
	const beamX = 30 + Math.random() * 40; // center of beam, 30-70%

	// Generate 8-14 motes
	const moteCount = 8 + Math.floor(Math.random() * 7);
	const motes = Array.from({ length: moteCount }, () => ({
		x: beamX - 8 + Math.random() * 16,
		y: 10 + Math.random() * 80,
		size: 1 + Math.random() * 1.5,
		driftX: -3 + Math.random() * 6,
		driftY: -4 + Math.random() * 8,
		duration: 6 + Math.random() * 8,
		delay: Math.random() * 6,
		opacity: 0.2 + Math.random() * 0.3,
	}));

	onMount(() => {
		requestAnimationFrame(() => { visible = true; });
	});
</script>

<div class="dust-motes" class:visible>
	<!-- Sunbeam stripe -->
	<div
		class="sunbeam"
		style:left="{beamX}%"
		style:--angle="{beamAngle}deg"
	></div>

	<!-- Motes -->
	{#each motes as mote}
		<div
			class="mote"
			style:left="{mote.x}%"
			style:top="{mote.y}%"
			style:width="{mote.size}px"
			style:height="{mote.size}px"
			style:--drift-x="{mote.driftX}px"
			style:--drift-y="{mote.driftY}px"
			style:--dur="{mote.duration}s"
			style:--delay="{mote.delay}s"
			style:--peak-opacity="{mote.opacity}"
		></div>
	{/each}
</div>

<style>
	.dust-motes {
		position: fixed;
		inset: 0;
		pointer-events: none;
		opacity: 0;
		transition: opacity 3s ease-in;
	}

	.dust-motes.visible {
		opacity: 1;
	}

	.sunbeam {
		position: absolute;
		top: -10%;
		width: 80px;
		height: 130%;
		background: linear-gradient(
			90deg,
			transparent 0%,
			rgba(255, 245, 200, 0.04) 30%,
			rgba(255, 245, 200, 0.06) 50%,
			rgba(255, 245, 200, 0.04) 70%,
			transparent 100%
		);
		transform: rotate(var(--angle));
		transform-origin: top center;
	}

	.mote {
		position: absolute;
		border-radius: 50%;
		background: rgba(255, 240, 200, 0.8);
		animation:
			mote-drift var(--dur) ease-in-out var(--delay) infinite alternate,
			mote-twinkle var(--dur) ease-in-out var(--delay) infinite;
	}

	@keyframes mote-drift {
		0% {
			transform: translate(0, 0);
		}
		33% {
			transform: translate(var(--drift-x), calc(var(--drift-y) * -0.5));
		}
		66% {
			transform: translate(calc(var(--drift-x) * -0.7), var(--drift-y));
		}
		100% {
			transform: translate(calc(var(--drift-x) * 0.3), calc(var(--drift-y) * -0.3));
		}
	}

	@keyframes mote-twinkle {
		0%, 100% { opacity: var(--peak-opacity); }
		30%      { opacity: calc(var(--peak-opacity) * 0.3); }
		60%      { opacity: var(--peak-opacity); }
		80%      { opacity: calc(var(--peak-opacity) * 0.5); }
	}
</style>
