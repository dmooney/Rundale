<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { get } from 'svelte/store';
	import type { NotebookAction } from '$lib/notebook/actions';
	import type { NpcInfo, TextLogEntry } from '$lib/types';
	import { submitInput } from '$lib/ipc';
	import { openBugReport } from '../../stores/bugReport';
	import { debugVisible } from '../../stores/debug';
	import {
		flushStream,
		formatIpcError,
		fullMapOpen,
		intentDraft,
		mapData,
		npcsHere,
		playerSubmittedCount,
		pushErrorLog,
		streamingActive,
		textLog,
		worldState,
	} from '../../stores/game';
	import { modSelectorVisible, savePickerVisible } from '../../stores/save';
	import {
		draftForNotebookAction,
		submitNotebookCommand,
	} from '$lib/illustrated-notebook/command';
	import { IllustratedNotebookRenderer } from '$lib/illustrated-notebook/renderer';
	import type { NotebookTab } from '$lib/illustrated-notebook/types';

	let hostEl: HTMLDivElement;
	let inputEl: HTMLInputElement;
	let renderer = $state<IllustratedNotebookRenderer | null>(null);
	let resizeObserver: ResizeObserver | null = null;
	let selectedRealName = $state<string | null>(null);
	let intentText = $state('');
	let inputFocused = $state(false);
	let isSubmitting = $state(false);
	let drawer = $state<NotebookTab | 'tools' | null>(null);

	const selectedNpc = $derived<NpcInfo | null>(
		$npcsHere.find((npc) => npc.real_name === selectedRealName) ??
			$npcsHere[0] ??
			null,
	);

	$effect(() => {
		if ($npcsHere.length === 0) {
			selectedRealName = null;
			return;
		}
		if (
			!selectedRealName ||
			!$npcsHere.some((npc) => npc.real_name === selectedRealName)
		) {
			selectedRealName = $npcsHere[0].real_name;
		}
	});

	$effect(() => {
		const draft = $intentDraft;
		if (draft === null) return;
		intentText = draft;
		intentDraft.set(null);
		focusInput();
	});

	$effect(() => {
		renderer?.render({
			world: $worldState,
			map: $mapData,
			npcs: $npcsHere,
			selectedNpc,
			selectedRealName,
			intentText,
			inputFocused,
			busy: $streamingActive || isSubmitting,
			callbacks: {
				onAction: seedAction,
				onFocusInput: focusInput,
				onOpenMap: () => fullMapOpen.set(true),
				onOpenTab: openTab,
				onSelectNpc: selectNpc,
				onSend: () => void submitCurrent(),
			},
		});
	});

	onMount(() => {
		let cancelled = false;
		void (async () => {
			const next = new IllustratedNotebookRenderer(hostEl);
			await next.init();
			if (cancelled) {
				next.destroy();
				return;
			}
			renderer = next;
			if (typeof ResizeObserver !== 'undefined') {
				resizeObserver = new ResizeObserver(() => renderer?.resize());
				resizeObserver.observe(hostEl);
			}
			renderer.resize();
			focusInput();
		})();
		return () => {
			cancelled = true;
		};
	});

	onDestroy(() => {
		resizeObserver?.disconnect();
		resizeObserver = null;
		renderer?.destroy();
		renderer = null;
	});

	function focusInput() {
		queueMicrotask(() => {
			inputEl?.focus({ preventScroll: true });
		});
	}

	function selectNpc(realName: string) {
		selectedRealName = realName;
	}

	function seedAction(action: NotebookAction) {
		intentText = draftForNotebookAction(action, selectedNpc);
		focusInput();
	}

	function openTab(tab: NotebookTab) {
		if (tab === 'places') {
			fullMapOpen.set(true);
			return;
		}
		drawer = tab;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (
			$streamingActive &&
			e.key !== 'Shift' &&
			e.key !== 'Control' &&
			e.key !== 'Alt' &&
			e.key !== 'Meta'
		) {
			get(flushStream)();
			if (e.key === 'Enter') {
				e.preventDefault();
				return;
			}
		}
		if (e.key === 'Enter') {
			e.preventDefault();
			void submitCurrent();
		}
	}

	async function submitCurrent() {
		const text = intentText;
		if (!text.trim() || isSubmitting || $streamingActive) return;
		isSubmitting = true;
		try {
			const didSubmit = await submitNotebookCommand({
				text,
				busy: false,
				paused: Boolean($worldState?.paused),
				submitInput: async (command) => {
					await submitInput(command);
				},
				onLocalSubmit: () => playerSubmittedCount.update((n) => n + 1),
			});
			if (didSubmit) intentText = '';
		} catch (err) {
			pushErrorLog(`Could not send input: ${formatIpcError(err)}`);
		} finally {
			isSubmitting = false;
		}
	}

	function recentLines(entries: TextLogEntry[]): TextLogEntry[] {
		return entries.slice(-8);
	}

	function openTools(which: 'save' | 'debug' | 'mod' | 'bug') {
		drawer = null;
		switch (which) {
			case 'save':
				savePickerVisible.set(true);
				break;
			case 'debug':
				debugVisible.update((v) => !v);
				break;
			case 'mod':
				modSelectorVisible.set(true);
				break;
			case 'bug':
				void openBugReport();
				break;
		}
	}
</script>

<section
	class="illustrated-notebook-game"
	data-testid="illustrated-notebook-game"
>
	<div
		bind:this={hostEl}
		class="pixi-host"
		data-testid="illustrated-notebook-pixi-host"
	></div>
	<input
		bind:this={inputEl}
		bind:value={intentText}
		class="notebook-native-input"
		type="text"
		aria-label="Player intent"
		aria-disabled={$streamingActive || isSubmitting}
		autocomplete="off"
		spellcheck="false"
		onfocus={() => (inputFocused = true)}
		onblur={() => (inputFocused = false)}
		onkeydown={handleKeydown}
	/>

	<button
		class="tools-hotspot"
		type="button"
		onclick={() => (drawer = 'tools')}
	>
		Notebook tools
	</button>

	{#if drawer}
		<aside class="notebook-drawer" aria-label={`${drawer} drawer`}>
			<header>
				<strong>{drawer === 'tools' ? 'Tools' : drawer}</strong>
				<button
					type="button"
					aria-label="Close notebook drawer"
					onclick={() => (drawer = null)}>Close</button
				>
			</header>
			{#if drawer === 'people'}
				<ul>
					{#each $npcsHere as npc (npc.real_name)}
						<li>
							<button
								type="button"
								onclick={() => {
									selectedRealName = npc.real_name;
									drawer = null;
								}}
							>
								{npc.name} <span>{npc.occupation}</span>
							</button>
						</li>
					{/each}
				</ul>
			{:else if drawer === 'journal' || drawer === 'notes'}
				<div class="journal-lines">
					{#each recentLines($textLog) as entry, i (`${entry.id ?? i}-${entry.content}`)}
						<p class:error={entry.subtype === 'error'}>
							<strong>{entry.source}</strong>: {entry.content}
						</p>
					{/each}
				</div>
			{:else if drawer === 'rumours'}
				<p>The parish has no pinned rumours in this notebook margin yet.</p>
			{:else if drawer === 'tools'}
				<div class="tool-grid">
					<button type="button" onclick={() => openTools('save')}
						>Save/Load</button
					>
					<button type="button" onclick={() => fullMapOpen.set(true)}
						>Map</button
					>
					<button type="button" onclick={() => openTools('debug')}>Debug</button
					>
					<button type="button" onclick={() => openTools('mod')}>Mod</button>
					<button type="button" onclick={() => openTools('bug')}
						>Bug Report</button
					>
				</div>
			{/if}
		</aside>
	{/if}
</section>

<style>
	.illustrated-notebook-game {
		position: relative;
		width: 100%;
		height: 100%;
		min-height: 100dvh;
		overflow: hidden;
		background: #21180f;
	}

	.pixi-host {
		position: absolute;
		inset: 0;
	}

	.pixi-host :global(canvas) {
		display: block;
		width: 100% !important;
		height: 100% !important;
	}

	.notebook-native-input {
		position: fixed;
		left: 1px;
		top: 1px;
		width: 1px;
		height: 1px;
		padding: 0;
		border: 0;
		opacity: 0;
		pointer-events: none;
	}

	.tools-hotspot {
		position: absolute;
		right: 0.5rem;
		top: 0.5rem;
		z-index: 3;
		width: 2.4rem;
		height: 2.4rem;
		overflow: hidden;
		text-indent: -999px;
		border: 0;
		background: transparent;
		cursor: pointer;
	}

	.notebook-drawer {
		position: absolute;
		right: min(1rem, 3vw);
		top: 5.2rem;
		z-index: 5;
		width: min(25rem, calc(100vw - 2rem));
		max-height: min(72vh, 38rem);
		overflow: auto;
		padding: 1rem;
		color: #322315;
		background:
			linear-gradient(rgba(246, 226, 180, 0.88), rgba(222, 195, 139, 0.92)),
			#ead3a0;
		border: 1px solid rgba(55, 38, 20, 0.5);
		box-shadow: 0 16px 40px rgba(20, 13, 6, 0.42);
		font-family: Georgia, 'Times New Roman', serif;
	}

	.notebook-drawer header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		border-bottom: 1px solid rgba(55, 38, 20, 0.35);
		padding-bottom: 0.45rem;
		margin-bottom: 0.6rem;
		text-transform: capitalize;
	}

	.notebook-drawer button {
		color: #322315;
		background: rgba(255, 246, 216, 0.52);
		border: 1px solid rgba(55, 38, 20, 0.38);
		padding: 0.35rem 0.55rem;
		font: inherit;
		cursor: pointer;
	}

	.notebook-drawer ul {
		list-style: none;
		padding: 0;
		margin: 0;
		display: grid;
		gap: 0.4rem;
	}

	.notebook-drawer li button {
		width: 100%;
		text-align: left;
	}

	.notebook-drawer li span {
		display: block;
		font-size: 0.82rem;
		opacity: 0.72;
	}

	.journal-lines {
		display: grid;
		gap: 0.45rem;
		font-size: 0.92rem;
	}

	.journal-lines p {
		margin: 0;
	}

	.journal-lines .error {
		color: #7e2f28;
	}

	.tool-grid {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 0.5rem;
	}

	@media (max-width: 760px) {
		.notebook-drawer {
			left: 0.75rem;
			right: 0.75rem;
			top: 11.6rem;
			width: auto;
			max-height: 48vh;
		}
	}
</style>
