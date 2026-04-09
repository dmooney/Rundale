<!--
  EffectsLayer — full-window overlay for ambient visual effects.

  Renders as a pointer-events:none div covering the entire viewport.
  Active effects are dispatched to individual renderer components
  based on their definition ID.

  The effects engine is initialised here and reads from the worldState
  store to evaluate conditions.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import { worldState, mapData } from '../stores/game';
	import { activeEffects, effectsEnabled } from '../stores/effects';
	import { EffectsEngine, EFFECT_DEFINITIONS } from '$lib/effects';
	import type { EffectContext } from '$lib/effects';

	import LightningFlash from './effects/LightningFlash.svelte';
	import RainStreaks from './effects/RainStreaks.svelte';
	import FogCreep from './effects/FogCreep.svelte';
	import FairySprite from './effects/FairySprite.svelte';
	import BogLights from './effects/BogLights.svelte';
	import TurfSmoke from './effects/TurfSmoke.svelte';

	let engine: EffectsEngine | null = null;

	/** Builds the current effect context from game stores. */
	function getContext(): EffectContext | null {
		const world = get(worldState);
		if (!world) return null;

		const map = get(mapData);
		// Determine indoor status from the current location in the map data
		let indoor = false;
		if (map) {
			const loc = map.locations.find((l) => l.id === map.player_location);
			if (loc?.indoor) indoor = true;
		}

		return {
			weather: world.weather,
			season: world.season,
			timeOfDay: world.time_label,
			hour: world.hour,
			locationName: world.location_name,
			indoor,
			festival: world.festival
		};
	}

	onMount(() => {
		engine = new EffectsEngine({
			onUpdate: (effects) => activeEffects.set(effects),
			getContext
		});

		engine.registerAll(EFFECT_DEFINITIONS);
		engine.start();

		// Sync enabled state with the store
		const unsub = effectsEnabled.subscribe((enabled) => {
			engine?.setEnabled(enabled);
		});

		return () => {
			unsub();
		};
	});

	onDestroy(() => {
		engine?.stop();
		engine = null;
	});
</script>

{#if $activeEffects.length > 0}
	<div class="effects-layer" aria-hidden="true">
		{#each $activeEffects as effect (effect.instanceKey)}
			{#if effect.id === 'lightning-flash'}<LightningFlash {effect} />{/if}
			{#if effect.id === 'rain-streaks'}<RainStreaks {effect} />{/if}
			{#if effect.id === 'fog-creep'}<FogCreep {effect} />{/if}
			{#if effect.id === 'fairy-sprite'}<FairySprite {effect} />{/if}
			{#if effect.id === 'bog-lights'}<BogLights {effect} />{/if}
			{#if effect.id === 'turf-smoke'}<TurfSmoke {effect} />{/if}
		{/each}
	</div>
{/if}

<style>
	.effects-layer {
		position: fixed;
		inset: 0;
		pointer-events: none;
		z-index: 400;
		overflow: hidden;
	}
</style>
