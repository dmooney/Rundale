<script lang="ts">
	import type { SceneHotspotView, SceneNpcView, SceneState } from '$lib/types';
	import { mapData, npcsHere, pushErrorLog, streamingActive, textLog, trimTextLog } from '../../stores/game';
	import { requestSceneNpcFocus } from '../../stores/scene';
	import { submitInput } from '$lib/ipc';
	import {
		activateSceneHotspot,
		activateSceneNpc,
		appendSceneInspectLog,
	} from '$lib/scene-actions';
	import ScenePlate from './ScenePlate.svelte';
	import SceneOverlay from './SceneOverlay.svelte';
	import HotspotLayer from './HotspotLayer.svelte';
	import NpcSpriteLayer from './NpcSpriteLayer.svelte';

	let {
		scene,
		travelPending = false,
		debugHotspots = false,
	}: {
		scene: SceneState;
		travelPending?: boolean;
		debugHotspots?: boolean;
	} = $props();

	function appendSystemLog(content: string) {
		textLog.update((log) => trimTextLog(appendSceneInspectLog(log, content)));
	}

	function handleHotspot(hotspot: SceneHotspotView) {
		if ($streamingActive) return;
		void activateSceneHotspot(hotspot.action, {
			mapData: $mapData,
			submitInput,
			appendSystemLog,
			onError: pushErrorLog,
			sceneNpcs: scene.npcs,
			npcsHere: $npcsHere,
			requestNpcFocus: requestSceneNpcFocus,
		});
	}

	function handleNpc(npc: SceneNpcView) {
		if ($streamingActive) return;
		activateSceneNpc(npc, {
			npcsHere: $npcsHere,
			requestNpcFocus: requestSceneNpcFocus,
		});
	}
</script>

<section
	class="diorama-view"
	class:travel-pending={travelPending}
	aria-label="{scene.location_name} scene"
	data-testid="diorama-view"
>
	<div class="scene-frame">
		<ScenePlate
			url={scene.plate_url}
			alt="{scene.location_name} scene plate"
			{travelPending}
		/>
		<SceneOverlay
			variant={scene.variant}
			weatherOverlay={scene.weather_overlay}
			indoor={scene.indoor}
		/>
		<NpcSpriteLayer
			npcs={scene.npcs}
			disabled={$streamingActive}
			onActivate={handleNpc}
		/>
		<HotspotLayer
			hotspots={scene.hotspots}
			debug={debugHotspots}
			disabled={$streamingActive}
			onActivate={handleHotspot}
		/>
	</div>
</section>

<style>
	.diorama-view {
		flex: 0 0 auto;
		padding: clamp(0.45rem, 1.4vw, 0.9rem);
		background: var(--color-bg);
		border-bottom: 1px solid var(--color-border);
	}

	.scene-frame {
		position: relative;
		aspect-ratio: 16 / 9;
		width: 100%;
		max-height: min(46vh, 34rem);
		overflow: hidden;
		background: var(--color-input-bg);
		box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-border) 80%, transparent);
	}

	.diorama-view.travel-pending .scene-frame::after {
		content: '';
		position: absolute;
		inset: 0;
		pointer-events: none;
		background: rgba(0, 0, 0, 0.12);
	}

	@media (max-width: 768px) {
		.diorama-view {
			padding: 0.4rem;
		}

		.scene-frame {
			max-height: 32vh;
		}
	}
</style>
