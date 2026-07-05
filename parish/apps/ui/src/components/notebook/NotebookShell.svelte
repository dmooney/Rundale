<script lang="ts">
	import { npcsHere } from '../../stores/game';
	import type { NpcInfo } from '$lib/types';
	import NotebookActionDesk from './NotebookActionDesk.svelte';
	import NotebookNearbyRail from './NotebookNearbyRail.svelte';
	import NotebookPage from './NotebookPage.svelte';
	import NotebookTopRibbon from './NotebookTopRibbon.svelte';
	import NotebookWorldStage from './NotebookWorldStage.svelte';

	let selectedRealName = $state<string | null>(null);

	const selectedNpc = $derived<NpcInfo | null>(
		$npcsHere.find((npc) => npc.real_name === selectedRealName) ?? $npcsHere[0] ?? null,
	);

	$effect(() => {
		if ($npcsHere.length === 0) {
			selectedRealName = null;
			return;
		}
		if (!selectedRealName || !$npcsHere.some((npc) => npc.real_name === selectedRealName)) {
			selectedRealName = $npcsHere[0].real_name;
		}
	});

	function selectPerson(realName: string) {
		selectedRealName = realName;
	}
</script>

<div class="notebook-shell" data-testid="parish-notebook-shell">
	<div class="scene-scrim" aria-hidden="true"></div>
	<NotebookTopRibbon />
	<NotebookWorldStage {selectedNpc} />
	<NotebookNearbyRail npcs={$npcsHere} {selectedRealName} onselect={selectPerson} />
	<NotebookPage {selectedNpc} />
	<NotebookActionDesk {selectedNpc} />
</div>

<style>
	.notebook-shell {
		--notebook-paper: #f3e6c5;
		--notebook-paper-deep: #dec894;
		--notebook-ink: #2f2417;
		--notebook-ink-soft: #6f5836;
		--notebook-wash-green: #697857;
		--notebook-wash-blue: #647789;

		position: relative;
		width: 100%;
		height: 100%;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
		background:
			linear-gradient(180deg, rgba(33, 27, 16, 0.08), rgba(33, 27, 16, 0.2)),
			url('/notebook-ui/scene-crossroads.png') center / cover no-repeat,
			#3a3523;
		color: var(--notebook-ink);
	}

	.notebook-shell::before {
		content: '';
		position: absolute;
		inset: 0;
		z-index: 0;
		pointer-events: none;
		background-image:
			linear-gradient(rgba(34, 25, 14, 0.055) 1px, transparent 1px),
			linear-gradient(90deg, rgba(34, 25, 14, 0.04) 1px, transparent 1px);
		background-size: 52px 52px, 52px 52px;
		mix-blend-mode: multiply;
	}

	.notebook-shell::after {
		content: '';
		position: absolute;
		inset: 0;
		z-index: 0;
		pointer-events: none;
		background:
			radial-gradient(ellipse at 50% 48%, transparent 48%, rgba(22, 17, 10, 0.42) 100%);
	}

	.scene-scrim {
		position: absolute;
		inset: 0;
		z-index: 1;
		pointer-events: none;
		background:
			linear-gradient(180deg, rgba(255, 238, 190, 0.06), transparent 25%),
			linear-gradient(90deg, rgba(16, 12, 7, 0.16), transparent 15%, transparent 74%, rgba(16, 12, 7, 0.18));
		mix-blend-mode: multiply;
	}

	@media (max-width: 900px) {
		.notebook-shell {
			overflow-y: auto;
			min-height: 100dvh;
			background-position: center top;
		}
	}
</style>
