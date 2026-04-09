<!--
  EffectsLayer — full-window overlay for ambient visual effects.

  Renders as a pointer-events:none div covering the entire viewport.
  Active effects are dispatched to individual renderer components
  based on their definition ID.

  The effects engine is initialised here and reads from the worldState
  store to evaluate conditions. Interaction-triggered effects (ink splash,
  page turn, candle gutter) are fired via the engine's trigger() method
  in response to game events.
-->
<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { get } from 'svelte/store';
	import { worldState, mapData, streamingActive } from '../stores/game';
	import { activeEffects, effectsEnabled, effectsEngine } from '../stores/effects';
	import { EffectsEngine, EFFECT_DEFINITIONS } from '$lib/effects';
	import type { EffectContext } from '$lib/effects';
	import { onTextLog, onTravelStart } from '$lib/ipc';

	// ── Effect renderers ────────────────────────────────────────────────
	// Weather
	import LightningFlash from './effects/LightningFlash.svelte';
	import RainStreaks from './effects/RainStreaks.svelte';
	import Drizzle from './effects/Drizzle.svelte';
	import FogCreep from './effects/FogCreep.svelte';
	import WindGust from './effects/WindGust.svelte';
	import RainInkBleed from './effects/RainInkBleed.svelte';
	import FrostCreep from './effects/FrostCreep.svelte';
	import MoonlitText from './effects/MoonlitText.svelte';
	// Folklore
	import FairySprite from './effects/FairySprite.svelte';
	import BogLights from './effects/BogLights.svelte';
	import VeilThins from './effects/VeilThins.svelte';
	import BansheeChill from './effects/BansheeChill.svelte';
	import HolyWellRadiance from './effects/HolyWellRadiance.svelte';
	// Living world
	import CrowOnBar from './effects/CrowOnBar.svelte';
	import PubCat from './effects/PubCat.svelte';
	import MothAtLamp from './effects/MothAtLamp.svelte';
	import SpiderThread from './effects/SpiderThread.svelte';
	import TurfSmoke from './effects/TurfSmoke.svelte';
	// Ambient
	import FirelightWarmth from './effects/FirelightWarmth.svelte';
	import DustMotes from './effects/DustMotes.svelte';
	import BreathingPage from './effects/BreathingPage.svelte';
	import AuroraBorealis from './effects/AuroraBorealis.svelte';
	import DawnShimmer from './effects/DawnShimmer.svelte';
	import LoughReeGlimmer from './effects/LoughReeGlimmer.svelte';
	// Seasonal
	import AutumnLeaf from './effects/AutumnLeaf.svelte';
	import SamhainCandles from './effects/SamhainCandles.svelte';
	import BealtaineSparks from './effects/BealtaineSparks.svelte';
	import ImbolcThaw from './effects/ImbolcThaw.svelte';
	import LughnasaGold from './effects/LughnasaGold.svelte';
	// Interaction-triggered
	import InkSplash from './effects/InkSplash.svelte';
	import PageTurn from './effects/PageTurn.svelte';
	import CandleGutter from './effects/CandleGutter.svelte';

	let engine: EffectsEngine | null = null;

	/** Builds the current effect context from game stores. */
	function getContext(): EffectContext | null {
		const world = get(worldState);
		if (!world) return null;

		const map = get(mapData);
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
		effectsEngine.set(engine);

		// Sync enabled state with the store
		const unsub = effectsEnabled.subscribe((enabled) => {
			engine?.setEnabled(enabled);
		});

		// ── Interaction triggers ──────────────────────────────────────────

		const listeners: Array<() => void> = [];

		// Ink splash on player input
		onTextLog((payload) => {
			if (payload.source === 'player') {
				engine?.trigger('ink-splash');
			}
		}).then((u) => listeners.push(u)).catch(() => {});

		// Page turn on travel
		onTravelStart(() => {
			engine?.trigger('page-turn');
		}).then((u) => listeners.push(u)).catch(() => {});

		// Candle gutter on idle (night only) — check every 30s
		let idleCheck: ReturnType<typeof setInterval> | null = null;
		let lastActivityAt = performance.now();
		const recordActivity = () => { lastActivityAt = performance.now(); };
		window.addEventListener('keydown', recordActivity);
		window.addEventListener('mousedown', recordActivity);

		idleCheck = setInterval(() => {
			const idle = performance.now() - lastActivityAt;
			if (idle > 120_000) {
				engine?.trigger('candle-gutter');
			}
		}, 30_000);

		return () => {
			unsub();
			listeners.forEach((fn) => fn());
			window.removeEventListener('keydown', recordActivity);
			window.removeEventListener('mousedown', recordActivity);
			if (idleCheck) clearInterval(idleCheck);
		};
	});

	onDestroy(() => {
		engine?.stop();
		engine = null;
		effectsEngine.set(null);
	});
</script>

{#if $activeEffects.length > 0}
	<div class="effects-layer" aria-hidden="true">
		{#each $activeEffects as effect (effect.instanceKey)}
			{#if effect.id === 'lightning-flash'}<LightningFlash {effect} />{/if}
			{#if effect.id === 'rain-streaks'}<RainStreaks {effect} />{/if}
			{#if effect.id === 'drizzle'}<Drizzle {effect} />{/if}
			{#if effect.id === 'fog-creep'}<FogCreep {effect} />{/if}
			{#if effect.id === 'wind-gust'}<WindGust {effect} />{/if}
			{#if effect.id === 'rain-ink-bleed'}<RainInkBleed {effect} />{/if}
			{#if effect.id === 'frost-creep'}<FrostCreep {effect} />{/if}
			{#if effect.id === 'moonlit-text'}<MoonlitText {effect} />{/if}
			{#if effect.id === 'fairy-sprite'}<FairySprite {effect} />{/if}
			{#if effect.id === 'bog-lights'}<BogLights {effect} />{/if}
			{#if effect.id === 'veil-thins'}<VeilThins {effect} />{/if}
			{#if effect.id === 'banshee-chill'}<BansheeChill {effect} />{/if}
			{#if effect.id === 'holy-well-radiance'}<HolyWellRadiance {effect} />{/if}
			{#if effect.id === 'crow-on-bar'}<CrowOnBar {effect} />{/if}
			{#if effect.id === 'pub-cat'}<PubCat {effect} />{/if}
			{#if effect.id === 'moth-at-lamp'}<MothAtLamp {effect} />{/if}
			{#if effect.id === 'spider-thread'}<SpiderThread {effect} />{/if}
			{#if effect.id === 'turf-smoke'}<TurfSmoke {effect} />{/if}
			{#if effect.id === 'firelight-warmth'}<FirelightWarmth {effect} />{/if}
			{#if effect.id === 'dust-motes'}<DustMotes {effect} />{/if}
			{#if effect.id === 'breathing-page'}<BreathingPage {effect} />{/if}
			{#if effect.id === 'aurora-borealis'}<AuroraBorealis {effect} />{/if}
			{#if effect.id === 'dawn-shimmer'}<DawnShimmer {effect} />{/if}
			{#if effect.id === 'lough-ree-glimmer'}<LoughReeGlimmer {effect} />{/if}
			{#if effect.id === 'autumn-leaf'}<AutumnLeaf {effect} />{/if}
			{#if effect.id === 'samhain-candles'}<SamhainCandles {effect} />{/if}
			{#if effect.id === 'bealtaine-sparks'}<BealtaineSparks {effect} />{/if}
			{#if effect.id === 'imbolc-thaw'}<ImbolcThaw {effect} />{/if}
			{#if effect.id === 'lughnasa-gold'}<LughnasaGold {effect} />{/if}
			{#if effect.id === 'ink-splash'}<InkSplash {effect} />{/if}
			{#if effect.id === 'page-turn'}<PageTurn {effect} />{/if}
			{#if effect.id === 'candle-gutter'}<CandleGutter {effect} />{/if}
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
