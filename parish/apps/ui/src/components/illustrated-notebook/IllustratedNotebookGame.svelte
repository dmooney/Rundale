<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { get } from 'svelte/store';
	import type { NotebookAction } from '$lib/notebook/actions';
	import type { NpcInfo, TextLogEntry } from '$lib/types';
	import { reactToMessage, submitInput } from '$lib/ipc';
	import { REACTION_PALETTE } from '$lib/reactions';
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
	import { buildNotebookSectionContent } from '$lib/illustrated-parish/sections';
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
		addReaction,
		playerSubmittedCount,
		pushErrorLog,
		removeReaction,
		streamingActive,
		textLog,
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
	let activeTab = $state<ParishTab>('notes');
	let intentText = $state('');
	let inputFocused = $state(false);
	let isSubmitting = $state(false);
	let commandError = $state<string | null>(null);
	let commandHistory = $state<string[]>(loadNotebookCommandHistory());
	let commandHistoryIndex = $state<number | null>(null);
	let commandHistoryDraft = $state('');
	let hitTargets = $state<ParishHitTarget[]>([]);
	let focusedTargetId = $state<string | null>(null);
	let reactionPickerMessageId = $state<string | null>(null);
	const pendingReactions = new SvelteSet<string>();

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
	const activeSection = $derived(
		buildNotebookSectionContent({
			activeTab,
			world: $worldState,
			map: $mapData,
			npcs: $npcsHere,
			selectedNpc,
			journalEntries: $textLog,
		}),
	);
	const latestReactableEntry = $derived(
		[...$textLog]
			.reverse()
			.find(
				(entry) =>
					Boolean(entry.id) &&
					!entry.streaming &&
					$npcsHere.some(
						(npc) =>
							npc.real_name === entry.source || npc.name === entry.source,
					),
			) ?? null,
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
			activeTab = 'people';
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
			activeTab,
			world: $worldState,
			map: $mapData,
			npcs: $npcsHere,
			selectedNpc,
			selectedRealName,
			journalEntries: $textLog,
			command: commandState,
			callbacks: {
				onAction: seedAction,
				onFocusInput: focusInput,
				onOpenSurface: openSurface,
				onOpenTab: openTab,
				onSelectNpc: selectNpc,
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
				// The harness must not infer readiness from document load or a timing
				// delay: assets and Pixi initialization are asynchronous. Announce only
				// after this renderer has composed a frame into its real canvas.
				requestAnimationFrame(() => {
					requestAnimationFrame(() => {
						// Pixi has completed initialization, `resize`, and two browser frames.
						// Do not use a document-level timer: this is tied to the renderer that
						// owns the surface the screenshot path will capture.
						if (hostEl?.querySelector('canvas')) {
							(window as typeof window & { __parishGraphicalFrameReady?: boolean })
								.__parishGraphicalFrameReady = true;
							window.dispatchEvent(new Event('parish:graphical-frame-ready'));
						}
					});
				});
				if (!get(notebookOverlay)) focusInput();
			} catch (error) {
				next?.destroy();
				if (!cancelled) {
					window.dispatchEvent(
						new CustomEvent('parish:graphical-frame-failed', {
							detail: formatIpcError(error),
						}),
					);
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
		activeTab = tab;
	}

	function selectNpc(realName: string) {
		selectedRealName = realName;
		activeTab = 'people';
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

	function reactionSpeaker(entry: TextLogEntry): string {
		const npc = $npcsHere.find(
			(candidate) =>
				candidate.real_name === entry.source || candidate.name === entry.source,
		);
		if (!npc) return 'a local';
		return npc.introduced
			? npc.name
			: `a ${npc.occupation?.trim() || 'local'}`;
	}

	function handleReaction(entry: TextLogEntry, emoji: string) {
		if (!entry.id) return;
		const key = `${entry.id}:${emoji}`;
		if (pendingReactions.has(key)) return;
		pendingReactions.add(key);
		const previousPlayerReaction = entry.reactions?.find(
			(reaction) => reaction.source === 'player',
		);
		addReaction(entry.id, emoji, 'player');
		const messageId = entry.id;
		reactToMessage(entry.source, entry.content.slice(0, 80), emoji)
			.catch((error) => {
				removeReaction(messageId, emoji, 'player');
				if (previousPlayerReaction) {
					addReaction(
						messageId,
						previousPlayerReaction.emoji,
						previousPlayerReaction.source,
					);
				}
				pushErrorLog(
					`Could not record reaction ${emoji}: ${formatIpcError(error)}`,
				);
			})
			.finally(() => pendingReactions.delete(key));
		reactionPickerMessageId = null;
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
			{$worldState?.time_label ?? 'Time unknown'}. Weather: {$worldState?.weather ??
				'unknown'}. Season: {$worldState?.season ?? 'unknown'}.
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
	<div
		class="notebook-screenreader-summary"
		role="region"
		aria-label="Notebook section"
		data-testid="notebook-active-section"
		data-section={activeSection.tab}
	>
		<h2>{activeSection.title}</h2>
		{#each activeSection.lines as line, index (index)}
			<p><strong>{line.label}:</strong> {line.text}</p>
		{/each}
	</div>
	{#if latestReactableEntry}
		<div
			class="notebook-reaction-strip"
			aria-label={`Reactions for ${reactionSpeaker(latestReactableEntry)}'s latest message`}
		>
			{#if (latestReactableEntry.reactions?.length ?? 0) > 0}
				<div class="reaction-bar" data-testid="reaction-bar">
					{#each latestReactableEntry.reactions ?? [] as reaction, index (`${reaction.emoji}:${reaction.source}:${index}`)}
						<span class="reaction-badge" title={reaction.source}>
							{reaction.emoji}
							{#if reaction.source !== 'player'}
								<span class="reaction-source">{reaction.source}</span>
							{/if}
						</span>
					{/each}
				</div>
			{/if}
			<button
				type="button"
				class="reaction-toggle"
				aria-label={`React to message from ${reactionSpeaker(latestReactableEntry)}`}
				onclick={() =>
					(reactionPickerMessageId =
						reactionPickerMessageId === latestReactableEntry.id
							? null
							: (latestReactableEntry.id ?? null))}
			>
				React
			</button>
			{#if reactionPickerMessageId === latestReactableEntry.id}
				<div
					class="reaction-picker"
					role="toolbar"
					aria-label="React to message"
					data-testid="reaction-picker"
				>
					{#each REACTION_PALETTE as reaction (reaction.emoji)}
						<button
							type="button"
							aria-label={`React with ${reaction.description}`}
							title={reaction.description}
							onclick={() =>
								handleReaction(latestReactableEntry, reaction.emoji)}
						>
							{reaction.emoji}
						</button>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
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
				aria-pressed={target.activation.type === 'open-tab'
					? target.activation.tab === activeTab
					: undefined}
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

	.notebook-reaction-strip {
		position: absolute;
		right: clamp(1rem, 22vw, 24rem);
		bottom: 5.25rem;
		z-index: 5;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: flex-end;
		gap: 0.3rem;
		max-width: min(34rem, 70vw);
		color: #322315;
		font: 0.78rem Georgia, 'Times New Roman', serif;
	}

	.reaction-bar,
	.reaction-picker {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: flex-end;
		gap: 0.3rem;
	}

	.reaction-badge,
	.notebook-reaction-strip button {
		flex-shrink: 0;
		white-space: nowrap;
		padding: 0.12rem 0.38rem;
		color: #322315;
		background: rgba(255, 246, 216, 0.88);
		border: 1px solid rgba(55, 38, 20, 0.4);
		border-radius: 999px;
		font: inherit;
	}

	.notebook-reaction-strip button {
		cursor: pointer;
	}

	.reaction-source {
		margin-left: 0.15rem;
		opacity: 0.75;
	}

	@media (max-width: 760px) {
		.notebook-reaction-strip {
			right: 0.75rem;
			bottom: 4.6rem;
			max-width: calc(100vw - 1.5rem);
		}
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
