<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { get } from 'svelte/store';
	import type { NotebookAction } from '$lib/notebook/actions';
	import type { NpcInfo } from '$lib/types';
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
	import {
		sortNotebookHitTargetsForFocus,
		type NotebookHitTarget,
	} from '$lib/illustrated-notebook/interactions';
	import { IllustratedNotebookRenderer } from '$lib/illustrated-notebook/renderer';
	import type { NotebookTab } from '$lib/illustrated-notebook/types';
	import {
		buildNotebookViewModel,
		notebookNpcLabel,
	} from '$lib/illustrated-notebook/view-model';

	let hostEl: HTMLDivElement;
	let inputEl: HTMLInputElement;
	let renderer = $state<IllustratedNotebookRenderer | null>(null);
	let resizeObserver: ResizeObserver | null = null;
	let selectedRealName = $state<string | null>(null);
	let intentText = $state('');
	let inputFocused = $state(false);
	let isSubmitting = $state(false);
	let drawer = $state<NotebookTab | 'tools' | 'time' | 'intents' | null>(null);
	let hitTargets = $state<NotebookHitTarget[]>([]);
	let focusedHitTargetId = $state<string | null>(null);

	const selectedNpc = $derived<NpcInfo | null>(
		$npcsHere.find((npc) => npc.real_name === selectedRealName) ??
			$npcsHere[0] ??
			null,
	);
	const focusableHitTargets = $derived(
		sortNotebookHitTargetsForFocus(hitTargets),
	);
	const notebookView = $derived(
		buildNotebookViewModel({
			world: $worldState,
			npcs: $npcsHere,
			selectedNpc,
			textLog: $textLog,
			busy: $streamingActive || isSubmitting,
			intentText,
		}),
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
			view: notebookView,
			intentText,
			inputFocused,
			busy: $streamingActive || isSubmitting,
			callbacks: {
				onAction: seedAction,
				onFocusInput: focusInput,
				onOpenActiveIntents: () => (drawer = 'intents'),
				onOpenMap: () => fullMapOpen.set(true),
				onOpenTab: openTab,
				onOpenTime: () => (drawer = 'time'),
				onSelectNpc: selectNpc,
				onSend: () => void submitCurrent(),
			},
		});
	});

	onMount(() => {
		let cancelled = false;
		void (async () => {
			let next: IllustratedNotebookRenderer | null = null;
			try {
				next = new IllustratedNotebookRenderer(hostEl, {
					onHitTargetsChanged: (targets) => {
						hitTargets = targets;
					},
				});
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
			} catch (err) {
				next?.destroy();
				if (!cancelled) {
					pushErrorLog(
						`Failed to initialize notebook renderer: ${formatIpcError(err)}`,
					);
				}
			}
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
		hitTargets = [];
		focusedHitTargetId = null;
	});

	function focusInput() {
		window.setTimeout(() => {
			inputEl?.focus({ preventScroll: true });
		}, 0);
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

	function focusHitTarget(id: string) {
		focusedHitTargetId = id;
		renderer?.setFocusedTarget(id);
	}

	function blurHitTarget(id: string) {
		if (focusedHitTargetId !== id) return;
		focusedHitTargetId = null;
		renderer?.setFocusedTarget(null);
	}

	function activateHitTarget(id: string) {
		renderer?.activateTarget(id);
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
	<div
		class="notebook-live-transcript"
		aria-label={notebookView.liveTitle}
		aria-live="polite"
	>
		{#if notebookView.liveLines.length === 0}
			<p>{notebookView.liveEmpty}</p>
		{:else}
			{#each notebookView.liveLines as line (line.key)}
				<p>{line.speaker}: {line.content}</p>
			{/each}
		{/if}
	</div>
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

	<div class="notebook-accessibility-targets" aria-label="Notebook controls">
		{#each focusableHitTargets as target (target.id)}
			<button
				type="button"
				class="notebook-accessibility-target"
				disabled={target.disabled}
				style={`left:${target.rect.x}px;top:${target.rect.y}px;width:${target.rect.width}px;height:${target.rect.height}px;`}
				aria-label={target.label}
				onfocus={() => focusHitTarget(target.id)}
				onblur={() => blurHitTarget(target.id)}
				onclick={() => activateHitTarget(target.id)}
			></button>
		{/each}
	</div>

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
								{notebookNpcLabel(npc)}
								<span>
									{npc.introduced
										? npc.occupation || 'occupation not recorded'
										: 'not yet introduced'}
								</span>
							</button>
						</li>
					{/each}
				</ul>
			{:else if drawer === 'journal' || drawer === 'notes'}
				<div class="journal-lines">
					{#each notebookView.liveLines as line (line.key)}
						<p class:error={line.kind === 'error'}>
							<strong>{line.speaker}</strong>: {line.content}
						</p>
					{/each}
					{#if notebookView.liveLines.length === 0}
						<p>{notebookView.liveEmpty}</p>
					{/if}
				</div>
			{:else if drawer === 'rumours'}
				<p>The parish has no pinned rumours in this notebook margin yet.</p>
			{:else if drawer === 'time'}
				<div class="journal-lines">
					<p>
						<strong>Clock</strong>:
						{String($worldState?.hour ?? 0).padStart(2, '0')}:{String(
							$worldState?.minute ?? 0,
						).padStart(2, '0')}
						{$worldState?.time_label ?? ''}
					</p>
					<p><strong>Weather</strong>: {$worldState?.weather ?? 'unknown'}</p>
					<p><strong>Season</strong>: {$worldState?.season ?? 'unknown'}</p>
				</div>
			{:else if drawer === 'intents'}
				<div class="journal-lines">
					<p><strong>Current line</strong>: {intentText || '(none)'}</p>
					<p>
						<strong>Parish reply</strong>: {$streamingActive
							? 'pending'
							: 'idle'}
					</p>
				</div>
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

	.notebook-live-transcript {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
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

	.notebook-accessibility-targets {
		position: absolute;
		inset: 0;
		z-index: 4;
		pointer-events: none;
	}

	.notebook-accessibility-target {
		position: absolute;
		padding: 0;
		border: 0;
		opacity: 0;
		background: transparent;
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
