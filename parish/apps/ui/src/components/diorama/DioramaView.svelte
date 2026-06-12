<script lang="ts">
	import type { SceneState, SceneNpcView, SceneAction } from '$lib/types';
	import ScenePlate from './ScenePlate.svelte';
	import HotspotLayer from './HotspotLayer.svelte';
	import NpcSpriteLayer from './NpcSpriteLayer.svelte';
	import SceneOverlay from './SceneOverlay.svelte';
	import { handleSceneAction, handleNpcSpriteClick } from '$lib/scene-actions';

	interface Props {
		sceneState: SceneState | null;
	}

	let { sceneState }: Props = $props();

	function onHotspotAction(action: SceneAction) {
		handleSceneAction(action);
	}

	function onSpriteClick(npc: SceneNpcView) {
		handleNpcSpriteClick(npc);
	}
</script>

{#if sceneState}
	<!-- Single aspect-ratio: 480/270 wrapper. All child layers use
	     position:absolute inset:0 so they're perfectly congruent. -->
	<div class="diorama-view" data-testid="diorama-view">
		<ScenePlate plate_url={sceneState.plate_url} />
		<HotspotLayer hotspots={sceneState.hotspots} onAction={onHotspotAction} />
		<NpcSpriteLayer npcs={sceneState.npcs} onSpriteClick={onSpriteClick} />
		<SceneOverlay />
	</div>
{/if}

<style>
	.diorama-view {
		position: relative;
		width: 100%;
		aspect-ratio: 480 / 270;
		overflow: hidden;
		background: var(--color-panel-bg, #1a2438);
		flex-shrink: 0;
	}
</style>
