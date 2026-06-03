<script lang="ts">
	import type { DebugSnapshot } from '$lib/types';
	import BugChip from './BugChip.svelte';

	let { snap }: { snap: DebugSnapshot } = $props();
</script>

<div class="section">
	<h4>Conversation Log ({snap.conversations.exchange_count})</h4>
	{#if snap.conversations.exchanges.length === 0}
		<div class="field muted">(no exchanges)</div>
	{:else}
		{#each [...snap.conversations.exchanges].reverse() as ex (ex.timestamp)}
			<div class="conv-entry">
				<div class="field muted">[{ex.timestamp}] @ {ex.location_name}<BugChip kind="conversation" label={`${ex.speaker_name} @ ${ex.timestamp}`} detail={ex} /></div>
				<div class="field">Player: {ex.player_input}</div>
				<div class="field accent">{ex.speaker_name}: {ex.npc_dialogue}</div>
			</div>
		{/each}
	{/if}
</div>

<style>
	.section { margin-bottom: 0.75rem; }
	.field { color: var(--color-fg); line-height: 1.4; word-break: break-word; }
	.accent { color: var(--color-accent); }
	.muted { color: var(--color-muted); }
	h4 { color: var(--color-accent); font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.08em; margin: 0 0 0.25rem; }

	.conv-entry {
		margin-bottom: 0.3rem;
		padding-bottom: 0.3rem;
		border-bottom: 1px dashed var(--color-border);
	}
</style>
