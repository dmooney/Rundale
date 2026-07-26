<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { get } from 'svelte/store';
	import type { NotebookAction } from '$lib/notebook/actions';
	import type { NpcInfo } from '$lib/types';
	import { reactToMessage, submitInput } from '$lib/ipc';
	import { REACTION_PALETTE } from '$lib/reactions';
	import { openBugReport } from '../../stores/bugReport';
	import { debugVisible } from '../../stores/debug';
	import {
		flushStream,
		formatIpcError,
		fullMapOpen,
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
	import { modSelectorVisible, savePickerVisible } from '../../stores/save';
	import {
		draftForNotebookAction,
		submitNotebookCommand,
	} from '$lib/illustrated-notebook/command';
	import {
		sortNotebookHitTargetsForFocus,
		type NotebookHitTarget,
	} from '$lib/illustrated-notebook/interactions';
	import { computeNotebookLayout } from '$lib/illustrated-notebook/layout';
	import { IllustratedNotebookRenderer } from '$lib/illustrated-notebook/renderer';
	import type {
		NotebookLiveLine,
		NotebookRect,
		NotebookTab,
	} from '$lib/illustrated-notebook/types';
	import {
		buildNotebookViewModel,
		notebookNpcLabel,
		notebookReactionSummary,
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
	let reactionPickerMessageId = $state<string | null>(null);
	let liveChronicleRect = $state<NotebookRect | null>(null);
	const pendingReactions = new SvelteSet<string>();

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
		}),
	);
	const reactionLine = $derived(
		[...notebookView.liveLines]
			.reverse()
			.find((line) => line.reactions.length > 0) ??
			[...notebookView.liveLines]
				.reverse()
				.find(
					(line) =>
						line.kind === 'npc' && Boolean(line.messageId) && !line.streaming,
				) ??
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
					resizeObserver = new ResizeObserver(() => {
						renderer?.resize();
						updateLiveChronicleRect();
					});
					resizeObserver.observe(hostEl);
				}
				renderer.resize();
				updateLiveChronicleRect();
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

	function updateLiveChronicleRect() {
		if (!hostEl) return;
		const layout = computeNotebookLayout(
			hostEl.clientWidth || window.innerWidth,
			hostEl.clientHeight || window.innerHeight,
		);
		liveChronicleRect = layout.liveChronicle;
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

	function handleReaction(line: NotebookLiveLine, emoji: string) {
		if (!line.messageId || line.kind !== 'npc') return;
		const entry = get(textLog).find((candidate) => candidate.id === line.messageId);
		if (!entry) return;
		const key = `${line.messageId}:${emoji}`;
		if (pendingReactions.has(key)) return;
		pendingReactions.add(key);
		const previousPlayerReaction = entry.reactions?.find(
			(reaction) => reaction.source === 'player',
		);
		addReaction(line.messageId, emoji, 'player');
		const messageId = line.messageId;
		reactToMessage(entry.source, entry.content.slice(0, 80), emoji)
			.catch((err) => {
				removeReaction(messageId, emoji, 'player');
				if (previousPlayerReaction) {
					addReaction(
						messageId,
						previousPlayerReaction.emoji,
						previousPlayerReaction.source,
					);
				}
				pushErrorLog(
					`Could not record reaction ${emoji}: ${formatIpcError(err)}`,
				);
			})
			.finally(() => pendingReactions.delete(key));
		reactionPickerMessageId = null;
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
				<p data-testid={line.kind === 'command' ? 'command-entry' : undefined}>
					{line.speaker}: {line.content}
					{#if line.reactions.length > 0}
						<span aria-label="Reactions">
							· {notebookReactionSummary(line.reactions)}
						</span>
					{/if}
				</p>
			{/each}
		{/if}
	</div>
	<p class="notebook-status-summary" data-testid="notebook-status-summary">
		Location: {notebookView.locationName}. Time: {notebookView.time}
		{$worldState?.time_label ?? ''}. Weather: {notebookView.weather}. Season:
		{$worldState?.season ?? 'unknown'}.
		{#if $worldState?.paused}Paused.{/if}
		{#if $worldState?.festival}Festival: {$worldState.festival}.{/if}
	</p>
	{#if reactionLine && liveChronicleRect}
		<div
			class="notebook-reaction-strip"
			class:player={reactionLine.kind === 'player' ||
				reactionLine.kind === 'command'}
			style={`left:${liveChronicleRect.x + 18}px;top:${liveChronicleRect.y + liveChronicleRect.height - 8}px;width:${Math.max(1, liveChronicleRect.width - 36)}px;`}
			aria-label={`Reactions for ${reactionLine.speaker}'s latest message`}
		>
			{#if reactionLine.reactions.length > 0}
				<div class="reaction-bar" data-testid="reaction-bar">
					{#each reactionLine.reactions as reaction (reaction.emoji + reaction.source)}
						<span class="reaction-badge" title={reaction.source}>
							<span>{reaction.emoji}</span>
							{#if reaction.source !== 'player'}
								<span class="reaction-source">{reaction.source}</span>
							{/if}
						</span>
					{/each}
				</div>
			{/if}
			{#if reactionLine.kind === 'npc' && reactionLine.messageId && !reactionLine.streaming}
				<button
					type="button"
					class="reaction-toggle"
					aria-label={`React to message from ${reactionLine.speaker}`}
					onmouseenter={() =>
						(reactionPickerMessageId = reactionLine.messageId)}
					onfocus={() => (reactionPickerMessageId = reactionLine.messageId)}
					onclick={() =>
						(reactionPickerMessageId =
							reactionPickerMessageId === reactionLine.messageId
								? null
								: reactionLine.messageId)}
				>
					React
				</button>
				{#if reactionPickerMessageId === reactionLine.messageId}
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
								onclick={() => handleReaction(reactionLine, reaction.emoji)}
							>
								{reaction.emoji}
							</button>
						{/each}
					</div>
				{/if}
			{/if}
		</div>
	{/if}
	<input
		bind:this={inputEl}
		bind:value={intentText}
		class="notebook-native-input"
		type="text"
		aria-label="Player intent"
		aria-busy={$streamingActive || isSubmitting}
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
		<aside
			class="notebook-drawer"
			aria-label={`${drawer === 'intents' ? 'active tasks' : drawer} drawer`}
		>
			<header>
				<strong
					>{drawer === 'tools'
						? 'Tools'
						: drawer === 'intents'
							? 'Active tasks'
							: drawer}</strong
				>
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
						<div class:player={line.kind === 'player' || line.kind === 'command'} class="journal-entry">
							<p
								class:error={line.kind === 'error'}
								class:command={line.kind === 'command'}
								data-testid={line.kind === 'command' ? 'command-entry' : undefined}
							>
								<strong>{line.speaker}</strong>: {line.content}
							</p>
							{#if line.reactions.length > 0}
								<div
									class="reaction-bar"
									data-testid="reaction-bar"
									aria-label={`Reactions to ${line.speaker}'s message`}
								>
									{#each line.reactions as reaction (reaction.emoji + reaction.source)}
										<span class="reaction-badge" title={reaction.source}>
											<span>{reaction.emoji}</span>
											{#if reaction.source !== 'player'}
												<span class="reaction-source">{reaction.source}</span>
											{/if}
										</span>
									{/each}
								</div>
							{/if}
							{#if line.kind === 'npc' && line.messageId && !line.streaming}
								<button
									type="button"
									class="reaction-toggle"
									aria-label={`React to message from ${line.speaker}`}
									onclick={() =>
										(reactionPickerMessageId =
											reactionPickerMessageId === line.messageId
												? null
												: line.messageId)}
								>
									React
								</button>
								{#if reactionPickerMessageId === line.messageId}
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
												onclick={() => handleReaction(line, reaction.emoji)}
											>
												{reaction.emoji}
											</button>
										{/each}
									</div>
								{/if}
							{/if}
						</div>
					{/each}
					{#if notebookView.liveLines.length === 0}
						<p>{notebookView.liveEmpty}</p>
					{/if}
					{#if drawer === 'notes' && ($worldState?.name_hints?.length ?? 0) > 0}
						<h4>Pronunciation notes</h4>
						<ul aria-label="Pronunciation notes">
							{#each $worldState?.name_hints ?? [] as hint (hint.word)}
								<li>
									<strong>{hint.word}</strong>
									<span>{hint.pronunciation}</span>
									{#if hint.meaning}<span>{hint.meaning}</span>{/if}
								</li>
							{/each}
						</ul>
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
					{#if $worldState?.paused}<p><strong>State</strong>: Paused</p>{/if}
					{#if $worldState?.festival}
						<p><strong>Festival</strong>: {$worldState.festival}</p>
					{/if}
				</div>
			{:else if drawer === 'intents'}
				<div class="journal-lines">
					{#if notebookView.activeTasks.length === 0}
						<p>No active task.</p>
					{:else}
						<ul class="task-list" aria-label="Active tasks">
							{#each notebookView.activeTasks as task (task.id)}
								<li>
									<strong>{task.description}</strong>
									<span>{task.statusLabel}</span>
								</li>
							{/each}
						</ul>
					{/if}
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

	.notebook-status-summary {
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

	.notebook-reaction-strip {
		position: absolute;
		z-index: 5;
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.35rem;
		transform: translateY(-100%);
		pointer-events: auto;
		color: #322315;
		font-family: Georgia, 'Times New Roman', serif;
		font-size: 0.78rem;
	}

	.notebook-reaction-strip.player {
		justify-content: flex-end;
	}

	.notebook-reaction-strip .reaction-bar {
		flex: 1 1 auto;
		min-width: 0;
	}

	.notebook-reaction-strip.player .reaction-bar {
		justify-content: flex-end;
	}

	.notebook-reaction-strip button {
		color: #322315;
		background: rgba(255, 246, 216, 0.88);
		border: 1px solid rgba(55, 38, 20, 0.42);
		border-radius: 999px;
		padding: 0.15rem 0.4rem;
		font: inherit;
		cursor: pointer;
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

	.journal-entry {
		display: grid;
		gap: 0.3rem;
	}

	.reaction-bar,
	.reaction-picker {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.3rem;
		justify-content: flex-start;
	}

	.journal-entry.player .reaction-bar {
		justify-content: flex-end;
	}

	.reaction-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		flex-shrink: 0;
		white-space: nowrap;
		padding: 0.1rem 0.35rem;
		border: 1px solid rgba(55, 38, 20, 0.35);
		border-radius: 999px;
		background: rgba(255, 246, 216, 0.48);
	}

	.reaction-source {
		font-size: 0.78rem;
		opacity: 0.78;
	}

	.reaction-toggle {
		justify-self: start;
		font-size: 0.78rem;
	}

	.journal-lines .error {
		color: #7e2f28;
	}

	.journal-lines .command {
		font-variant: small-caps;
		letter-spacing: 0.035em;
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
