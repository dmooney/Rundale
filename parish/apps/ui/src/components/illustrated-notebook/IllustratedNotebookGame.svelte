<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { get } from 'svelte/store';
	import type { NotebookAction } from '$lib/notebook/actions';
	import type { NpcInfo } from '$lib/types';
	import { submitInput } from '$lib/ipc';
	import {
		appendNotebookCommandHistory,
		draftForNotebookAction,
		loadNotebookCommandHistory,
		resolveNotebookCommandPresentation,
		saveNotebookCommandHistory,
		submitNotebookCommand,
	} from '$lib/illustrated-notebook/command';
	import { sortParishTargetsForFocus } from '$lib/illustrated-parish/interactions';
	import { IllustratedParishRenderer } from '$lib/illustrated-parish/renderer';
	import type {
		NotebookCommandState,
		NotebookSurface,
		ParishHitTarget,
		ParishTab,
	} from '$lib/illustrated-parish/types';
	import {
		flushStream,
		formatIpcError,
		intentDraft,
		mapData,
		npcsHere,
		playerSubmittedCount,
		pushErrorLog,
		streamingActive,
		worldState,
	} from '../../stores/game';
	import {
		notebookPersonSelection,
		notebookOverlay,
		notebookOverlayTransitioning,
		openNotebookOverlay,
	} from '../../stores/notebookOverlay';

	let hostEl: HTMLDivElement;
	let inputEl: HTMLInputElement;
	let renderer = $state<IllustratedParishRenderer | null>(null);
	let resizeObserver: ResizeObserver | null = null;
	let selectedRealName = $state<string | null>(null);
	let intentText = $state('');
	let inputFocused = $state(false);
	let isSubmitting = $state(false);
	let commandError = $state<string | null>(null);
	let commandHistory = $state<string[]>(loadNotebookCommandHistory());
	let commandHistoryIndex = $state<number | null>(null);
	let commandHistoryDraft = $state('');
	let hitTargets = $state<ParishHitTarget[]>([]);
	let focusedTargetId = $state<string | null>(null);

	const selectedNpc = $derived<NpcInfo | null>(
		$npcsHere.find((npc) => npc.real_name === selectedRealName) ??
			$npcsHere[0] ??
			null,
	);
	const focusableHitTargets = $derived(sortParishTargetsForFocus(hitTargets));
	const notebookBlocked = $derived(
		$notebookOverlay !== null || $notebookOverlayTransitioning,
	);
	const commandState = $derived<NotebookCommandState>({
		text: intentText,
		focused: inputFocused,
		busy: $streamingActive,
		disabled: isSubmitting,
		error: commandError,
	});
	const commandPresentation = $derived(
		resolveNotebookCommandPresentation(commandState),
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
		const requestedPerson = $notebookPersonSelection;
		if (!requestedPerson) return;
		if ($npcsHere.some((npc) => npc.real_name === requestedPerson)) {
			selectedRealName = requestedPerson;
		}
		notebookPersonSelection.set(null);
	});

	$effect(() => {
		const draft = $intentDraft;
		if (draft === null) return;
		intentText = draft;
		resetCommandHistoryNavigation();
		intentDraft.set(null);
		if (!$notebookOverlay) focusInput();
	});

	$effect(() => {
		renderer?.render({
			world: $worldState,
			map: $mapData,
			npcs: $npcsHere,
			selectedNpc,
			selectedRealName,
			command: commandState,
			callbacks: {
				onAction: seedAction,
				onFocusInput: focusInput,
				onOpenSurface: openSurface,
				onOpenTab: openTab,
				onSelectNpc: (realName) => (selectedRealName = realName),
				onSend: () => void submitCurrent(),
			},
		});
	});

	onMount(() => {
		let cancelled = false;
		void (async () => {
			let next: IllustratedParishRenderer | null = null;
			try {
				next = new IllustratedParishRenderer(hostEl, {
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
				if (!get(notebookOverlay)) focusInput();
			} catch (error) {
				next?.destroy();
				if (!cancelled) {
					pushErrorLog(
						`Failed to initialize illustrated parish: ${formatIpcError(error)}`,
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
		focusedTargetId = null;
	});

	function focusInput() {
		if (get(notebookOverlay)) return;
		window.setTimeout(() => inputEl?.focus({ preventScroll: true }), 0);
	}

	function seedAction(action: NotebookAction) {
		if (isSubmitting) return;
		commandError = null;
		intentText = draftForNotebookAction(action, selectedNpc);
		resetCommandHistoryNavigation();
		focusInput();
	}

	function clearCommandError() {
		commandError = null;
	}

	function resetCommandHistoryNavigation() {
		commandHistoryIndex = null;
		commandHistoryDraft = '';
	}

	function recordCommandHistory(command: string) {
		commandHistory = appendNotebookCommandHistory(commandHistory, command);
		saveNotebookCommandHistory(commandHistory);
		resetCommandHistoryNavigation();
	}

	function handleCommandInput() {
		clearCommandError();
		resetCommandHistoryNavigation();
	}

	function handleCommandHistory(event: KeyboardEvent): boolean {
		if (
			(event.key !== 'ArrowUp' && event.key !== 'ArrowDown') ||
			event.altKey ||
			event.ctrlKey ||
			event.metaKey ||
			isSubmitting
		) {
			return false;
		}

		if (event.key === 'ArrowUp') {
			if (commandHistory.length === 0) return false;
			event.preventDefault();
			if (commandHistoryIndex === null) {
				commandHistoryDraft = intentText;
				commandHistoryIndex = commandHistory.length - 1;
			} else if (commandHistoryIndex > 0) {
				commandHistoryIndex -= 1;
			}
			intentText = commandHistory[commandHistoryIndex] ?? '';
			clearCommandError();
			return true;
		}

		if (commandHistoryIndex === null) return false;
		event.preventDefault();
		if (commandHistoryIndex < commandHistory.length - 1) {
			commandHistoryIndex += 1;
			intentText = commandHistory[commandHistoryIndex] ?? '';
		} else {
			intentText = commandHistoryDraft;
			resetCommandHistoryNavigation();
		}
		clearCommandError();
		return true;
	}

	function openTab(tab: ParishTab) {
		const surface: NotebookSurface =
			tab === 'people'
				? 'people'
				: tab === 'places'
					? 'map'
					: tab === 'rumours'
						? 'rumours'
						: 'journal';
		openSurface(surface);
	}

	function openSurface(surface: NotebookSurface) {
		void openNotebookOverlay(surface);
	}

	function focusHitTarget(id: string) {
		focusedTargetId = id;
		renderer?.setFocusedTarget(id);
	}

	function blurHitTarget(id: string) {
		if (focusedTargetId !== id) return;
		focusedTargetId = null;
		renderer?.setFocusedTarget(null);
	}

	function handleKeydown(event: KeyboardEvent) {
		if (
			$streamingActive &&
			event.key !== 'Shift' &&
			event.key !== 'Control' &&
			event.key !== 'Alt' &&
			event.key !== 'Meta'
		) {
			get(flushStream)();
			if (event.key === 'Enter') {
				event.preventDefault();
			}
			return;
		}
		if (handleCommandHistory(event)) return;
		if (event.key === 'Enter') {
			event.preventDefault();
			void submitCurrent();
		}
	}

	async function submitCurrent() {
		const text = intentText;
		if (!text.trim() || isSubmitting || $streamingActive) return;
		commandError = null;
		isSubmitting = true;
		try {
			const didSubmit = await submitNotebookCommand({
				text,
				busy: false,
				paused: Boolean($worldState?.paused),
				submitInput: async (command) => {
					await submitInput(command);
				},
				onLocalSubmit: () => playerSubmittedCount.update((count) => count + 1),
			});
			if (didSubmit) {
				recordCommandHistory(text);
				intentText = '';
			}
		} catch (err) {
			commandError = `Could not send input: ${formatIpcError(err)}`;
			pushErrorLog(commandError);
		} finally {
			isSubmitting = false;
		}
	}
</script>

<section
	class="illustrated-notebook-game"
	class:overlay-open={notebookBlocked}
	data-testid="illustrated-notebook-game"
	aria-label="Rundale illustrated parish notebook"
	aria-hidden={notebookBlocked}
	inert={notebookBlocked}
>
	<div
		bind:this={hostEl}
		class="pixi-host"
		data-testid="illustrated-notebook-pixi-host"
		aria-hidden="true"
	></div>
	<div
		class="notebook-screenreader-summary"
		role="status"
		aria-label="Parish status"
		aria-live="polite"
		aria-atomic="true"
	>
		<p>
			Location: {$worldState?.location_name ?? 'unknown'}.
			{$worldState?.time_label ?? 'Time unknown'}.
			Weather: {$worldState?.weather ?? 'unknown'}.
			Season: {$worldState?.season ?? 'unknown'}.
			{#if $worldState?.paused}The parish clock is paused.{/if}
			{#if $worldState?.festival}Festival: {$worldState.festival}.{/if}
		</p>
		<p>
			{#if selectedNpc}
				Selected person: {selectedNpc.name}, {selectedNpc.occupation ||
					'parish resident'}, mood {selectedNpc.mood || 'watchful'}.
			{:else}
				No one is nearby.
			{/if}
			{$streamingActive || isSubmitting
				? 'The parish is preparing a reply.'
				: 'Ready for your intent.'}
		</p>
	</div>
	<input
		bind:this={inputEl}
		bind:value={intentText}
		class="notebook-native-input"
		type="text"
		aria-label="Player intent"
		aria-disabled={isSubmitting || undefined}
		aria-busy={$streamingActive || isSubmitting}
		aria-invalid={Boolean(commandError)}
		aria-describedby={commandError ? 'notebook-command-status' : undefined}
		data-command-state={commandPresentation.phase}
		readonly={isSubmitting}
		autocomplete="off"
		spellcheck="false"
		oninput={handleCommandInput}
		onfocus={() => (inputFocused = true)}
		onblur={() => (inputFocused = false)}
		onkeydown={handleKeydown}
	/>
	<span
		id="notebook-command-status"
		class="notebook-command-status"
		role={commandError ? 'alert' : 'status'}
		aria-label="Command status"
	>
		{commandPresentation.statusText ?? 'Command line ready'}
	</span>

	<nav class="notebook-accessibility-targets" aria-label="Notebook controls">
		{#each focusableHitTargets as target (target.id)}
			<button
				type="button"
				class="notebook-accessibility-target"
				disabled={target.disabled}
				style={`left:${target.rect.x}px;top:${target.rect.y}px;width:${target.rect.width}px;height:${target.rect.height}px;`}
				aria-label={target.label}
				onfocus={() => focusHitTarget(target.id)}
				onblur={() => blurHitTarget(target.id)}
				onclick={() => renderer?.activateTarget(target.id)}
			></button>
		{/each}
	</nav>
</section>

<style>
	.illustrated-notebook-game {
		position: relative;
		width: 100%;
		height: 100%;
		min-height: 100dvh;
		overflow: hidden;
		background: #302b22;
	}

	.illustrated-notebook-game.overlay-open {
		pointer-events: none;
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

	.notebook-command-status {
		position: fixed;
		left: 1px;
		top: 1px;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip-path: inset(50%);
		white-space: nowrap;
		border: 0;
	}

	.notebook-screenreader-summary {
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
</style>
