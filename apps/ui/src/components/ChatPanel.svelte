<script lang="ts">
	import { tick } from 'svelte';
	import { textLog, streamingActive, loadingPhrase, loadingColor, addReaction, messageHints, worldState, nameHints } from '../stores/game';
	import type { TextLogEntry } from '$lib/types';
	import { REACTION_PALETTE } from '$lib/reactions';
	import { reactToMessage } from '$lib/ipc';
	import { segmentText } from '$lib/rich-text';

	let logEl: HTMLDivElement;
	let hoveredMessageId: string | null = $state(null);

	$effect(() => {
		const _ = $textLog;
		const nearBottom = logEl
			? logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight < 50
			: true;
		tick().then(() => {
			if (logEl && nearBottom) {
				logEl.scrollTop = logEl.scrollHeight;
			}
		});
	});

	function entryType(entry: TextLogEntry): 'player' | 'npc' | 'system' {
		if (entry.source === 'player') return 'player';
		if (entry.source === 'system') return 'system';
		return 'npc';
	}

	function displayLabel(entry: TextLogEntry): string {
		if (entry.source === 'player') return 'You';
		return entry.source;
	}

	interface TextSegment {
		text: string;
		isAction: boolean;
	}

	interface RenderSegment extends TextSegment {
		animate?: boolean;
		animationKey?: number;
	}

	/** Splits text on *action* markers into normal and emote segments. */
	function parseEmotes(content: string): TextSegment[] {
		const segments: TextSegment[] = [];
		const regex = /\*([^*]+)\*/g;
		let lastIndex = 0;
		let match: RegExpExecArray | null;
		while ((match = regex.exec(content)) !== null) {
			if (match.index > lastIndex) {
				segments.push({ text: content.slice(lastIndex, match.index), isAction: false });
			}
			segments.push({ text: match[1], isAction: true });
			lastIndex = regex.lastIndex;
		}
		if (lastIndex < content.length) {
			segments.push({ text: content.slice(lastIndex), isAction: false });
		}
		// If no emotes found, return the whole content as a single segment
		if (segments.length === 0) {
			segments.push({ text: content, isAction: false });
		}
		return segments;
	}

	function renderSegments(entry: TextLogEntry): RenderSegment[] {
		const segments = parseEmotes(entry.content);
		const latestChunk = entry.streaming ? entry.latest_chunk : null;
		if (!latestChunk) return segments;

		for (let index = segments.length - 1; index >= 0; index -= 1) {
			const segment = segments[index];
			if (!segment.text.endsWith(latestChunk)) continue;

			const stableText = segment.text.slice(0, -latestChunk.length);
			const leadingWhitespace = latestChunk.match(/^\s+/u)?.[0] ?? '';
			const trailingWhitespace = latestChunk.match(/\s+$/u)?.[0] ?? '';
			const animatedText = latestChunk.slice(
				leadingWhitespace.length,
				latestChunk.length - trailingWhitespace.length
			);
			const animatedSegments: RenderSegment[] = [];
			if (stableText) {
				animatedSegments.push({ text: stableText, isAction: segment.isAction });
			}
			if (leadingWhitespace) {
				animatedSegments.push({ text: leadingWhitespace, isAction: segment.isAction });
			}
			if (animatedText) {
				animatedSegments.push({
					text: animatedText,
					isAction: segment.isAction,
					animate: true,
					animationKey: entry.stream_chunk_id ?? entry.content.length
				});
			}
			if (trailingWhitespace) {
				animatedSegments.push({ text: trailingWhitespace, isAction: segment.isAction });
			}

			return [...segments.slice(0, index), ...animatedSegments];
		}

		return segments;
	}

	/** Returns rich text segments for a piece of message text, annotating
	 *  Irish words (per message), names, and location name. */
	function richify(text: string, entryId?: string) {
		const hints = (entryId ? $messageHints.get(entryId) : undefined) ?? [];
		const irishWords = hints.map((h) => h.word);
		const names = $nameHints.map((h) => h.word);
		const location = $worldState?.location_name ?? '';
		return segmentText(text, irishWords, names, location);
	}

	function handleReaction(entry: TextLogEntry, emoji: string) {
		if (!entry.id) return;
		// Optimistic UI update
		addReaction(entry.id, emoji, 'player');
		// Send to backend
		const snippet = entry.content.slice(0, 80);
		reactToMessage(entry.source, snippet, emoji).catch(() => {});
		// Close picker
		hoveredMessageId = null;
	}
</script>

<div class="chat-panel" data-testid="chat-panel" bind:this={logEl} role="log" aria-live="polite" aria-label="Game chat log">
	{#each $textLog as entry, index (entry.id ?? entry.stream_turn_id ?? `${entry.source}:${index}`)}
		{#if entryType(entry) === 'system'}
			{@const isSplash = entry.content.includes('Copyright \u00A9')}
			{@const lines = entry.content.split('\n')}
			<div class="entry system" class:location={entry.subtype === 'location'}>
				{#if isSplash}
					<span class="content"><strong>{lines[0]}</strong>{'\n' + lines.slice(1).join('\n')}</span>
				{:else}
					<span class="content">{#each parseEmotes(entry.content) as seg}{#if seg.isAction}<span class="emote">{seg.text}</span>{:else}{#each richify(seg.text) as rs}<span class="term-{rs.kind}">{rs.text}</span>{/each}{/if}{/each}</span>
				{/if}
			</div>
		{:else}
			<div class="bubble-row {entryType(entry)}">
				<div class="bubble-wrapper">
					<span class="label">{displayLabel(entry)}</span>
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div
						class="bubble-anchor"
						onmouseenter={() => { if (entryType(entry) === 'npc' && !entry.streaming && entry.id) hoveredMessageId = entry.id ?? null; }}
						onmouseleave={() => { hoveredMessageId = null; }}
					>
						<div class="bubble">
							<span class="content"
								>{#each renderSegments(entry) as seg}{#if seg.animate}{#key seg.animationKey}<span class="stream-chunk" class:emote={seg.isAction}>{seg.text}</span>{/key}{:else if seg.isAction}<span class="emote">{seg.text}</span>{:else}{#each richify(seg.text, entry.id) as rs}<span class="term-{rs.kind}">{rs.text}</span>{/each}{/if}{/each}</span>
						</div>

						<!-- Reaction picker (floats over bubble, NPC messages only) -->
						{#if hoveredMessageId && hoveredMessageId === entry.id && entryType(entry) === 'npc'}
							<div class="reaction-picker" role="toolbar" aria-label="React to message" data-testid="reaction-picker">
								{#each REACTION_PALETTE as reaction}
									<button
										type="button"
										class="reaction-btn"
										title={reaction.description}
										aria-label={`React with ${reaction.description}`}
										onclick={() => handleReaction(entry, reaction.emoji)}
									>
										<span aria-hidden="true">{reaction.emoji}</span>
									</button>
								{/each}
							</div>
						{/if}
					</div>

					<!-- Existing reactions -->
					{#if entry.reactions && entry.reactions.length > 0}
						<div class="reaction-bar" data-testid="reaction-bar">
							{#each entry.reactions as r}
								<span class="reaction-badge" title={r.source}>
									{r.emoji}
									{#if r.source !== 'player'}
										<span class="reaction-source">{r.source}</span>
									{/if}
								</span>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		{/if}
	{/each}
	{#if $streamingActive && ($textLog.length === 0 || !$textLog[$textLog.length - 1].streaming)}
		<div class="loading-row" role="status" aria-label="Generating response">
			<svg class="triquetra-spinner" viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
				<circle class="knot-circle" pathLength="120"
					cx="50" cy="50" r="16"
					fill="none" stroke="var(--color-accent)" stroke-width="3"
					stroke-linecap="round" />
				<path class="triquetra-path" pathLength="120"
					d="M 50 22
					   A 28 28 0 0 0 74.25 64
					   A 28 28 0 0 0 25.75 64
					   A 28 28 0 0 0 50 22 Z"
					fill="none" stroke="var(--color-accent)" stroke-width="3"
					stroke-linecap="round" stroke-linejoin="round" />
			</svg>
			<span class="loading-phrase" style="color: rgb({$loadingColor[0]}, {$loadingColor[1]}, {$loadingColor[2]})">{$loadingPhrase}</span>
		</div>
	{/if}
</div>

<style>
	.chat-panel {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		overscroll-behavior: contain;
		-webkit-overflow-scrolling: touch;
		padding: 1rem;
		display: flex;
		flex-direction: column;
		justify-content: flex-end;
		gap: 0.6rem;
		background: var(--color-bg);
	}

	/* System messages: narrative prose */
	.entry.system {
		line-height: 1.75;
		font-size: 1.05rem;
		color: var(--color-fg);
		white-space: pre-wrap;
		padding: 0.65rem 0;
	}

	/* Location description: subtle left border in location yellow */
	.entry.system.location {
		border-left: 3px solid var(--color-location);
		padding-left: 0.75rem;
		color: var(--color-muted);
	}

	/* Inline term highlighting */
	:global(.term-irish)    { color: var(--color-irish); }
	:global(.term-name)     { color: var(--color-name); }
	:global(.term-location) { color: var(--color-location); font-style: italic; }

	/* Title card: splash message with <strong> title */
	.entry.system :global(strong) {
		font-family: var(--font-display);
		font-size: 1.25rem;
		letter-spacing: 0.06em;
		display: block;
		color: var(--color-accent);
		font-weight: 600;
		margin-bottom: 0.4rem;
		text-align: center;
	}

	/* Bubble row: flex container controlling left/right alignment */
	.bubble-row {
		display: flex;
		width: 100%;
	}

	.bubble-row.npc {
		justify-content: flex-start;
	}

	.bubble-row.player {
		justify-content: flex-end;
	}

	/* Wrapper keeps label + bubble aligned together */
	.bubble-wrapper {
		display: flex;
		flex-direction: column;
		max-width: 75%;
	}

	/* Name labels — Cinzel small caps */
	.label {
		font-family: var(--font-display);
		font-size: 0.66rem;
		font-weight: 600;
		letter-spacing: 0.1em;
		margin-bottom: 0.2rem;
	}

	.npc .label {
		color: var(--color-accent);
		text-align: left;
		padding-left: 0.75rem;
	}

	.player .label {
		color: var(--color-muted);
		text-align: right;
		padding-right: 0.5rem;
	}

	/* NPC message: dialogue leaf — left accent border, no rounded top-left */
	.npc .bubble {
		background: var(--color-panel-bg);
		color: var(--color-fg);
		border-radius: 0 0.85rem 0.85rem 0.15rem;
		border-left: 3px solid var(--color-accent);
		font-style: italic;
		padding: 0.6rem 0.9rem 0.6rem 0.85rem;
		font-size: 1.1rem;
		line-height: 1.6;
		white-space: pre-wrap;
		word-wrap: break-word;
	}

	/* Player message: italic, no rounded top-right */
	.player .bubble {
		background: var(--color-accent);
		color: var(--color-bg);
		border-radius: 0.85rem 0 0.15rem 0.85rem;
		font-style: italic;
		padding: 0.6rem 0.9rem;
		font-size: 1.05rem;
		line-height: 1.5;
		white-space: pre-wrap;
		word-wrap: break-word;
	}

	.emote {
		font-style: italic;
		opacity: 0.85;
	}

	.stream-chunk {
		display: inline-block;
		white-space: pre-wrap;
		will-change: clip-path, opacity;
		animation: stream-chunk-sweep 240ms linear forwards;
	}

	@keyframes stream-chunk-sweep {
		from {
			opacity: 0.24;
			clip-path: inset(0 100% 0 0);
		}
		to {
			opacity: 1;
			clip-path: inset(0 0 0 0);
		}
	}

	/* Bubble anchor: positioning context for the floating reaction picker */
	.bubble-anchor {
		position: relative;
		width: fit-content;
	}

	/* Reaction picker: floats over the bottom edge of the bubble */
	.reaction-picker {
		position: absolute;
		top: calc(100% - 10px);
		left: 0;
		z-index: 10;
		display: flex;
		gap: 0.15rem;
		padding: 0.2rem 0.25rem;
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25);
		width: fit-content;
	}

	.reaction-btn {
		background: none;
		border: none;
		padding: 0.15rem 0.2rem;
		font-size: 0.85rem;
		cursor: pointer;
		border-radius: 4px;
		line-height: 1;
		transition: transform 0.1s, background 0.1s;
	}

	.reaction-btn:hover {
		transform: scale(1.3);
		background: var(--color-input-bg);
	}

	/* Reaction bar (displayed reactions) */
	.reaction-bar {
		display: flex;
		gap: 0.25rem;
		margin-top: 0.2rem;
		flex-wrap: wrap;
	}

	.reaction-badge {
		display: inline-flex;
		align-items: center;
		gap: 0.15rem;
		font-size: 0.75rem;
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		padding: 0.1rem 0.35rem;
	}

	.reaction-source {
		font-size: 0.65rem;
		color: var(--color-muted);
	}

	.loading-row {
		display: flex;
		align-items: center;
		gap: 0.65rem;
		padding: 0.5rem 0;
		font-size: 1.05rem;
		animation: fade-in 0.4s ease-in;
	}

	.loading-phrase {
		font-style: italic;
		font-family: var(--font-body);
		letter-spacing: 0.01em;
		transition: color 0.5s ease;
	}

	@keyframes fade-in {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.triquetra-spinner {
		width: 2.5rem;
		height: 2.5rem;
		animation: triquetra-rotate 6s linear infinite;
	}

	.triquetra-path {
		stroke-dasharray: 80 40;
		stroke-dashoffset: 0;
		animation: triquetra-draw 2.4s linear infinite;
	}

	.knot-circle {
		stroke-dasharray: 0 120;
		stroke-dashoffset: 0;
		animation: circle-draw 3s ease-in-out infinite;
		animation-delay: 0.4s;
	}

	@keyframes triquetra-draw {
		to {
			stroke-dashoffset: -120;
		}
	}

	@keyframes circle-draw {
		0%   { stroke-dasharray: 0 120;   stroke-dashoffset: 0; }
		30%  { stroke-dasharray: 120 120; stroke-dashoffset: 0; }
		70%  { stroke-dasharray: 120 120; stroke-dashoffset: 0; }
		100% { stroke-dasharray: 0 120;   stroke-dashoffset: -120; }
	}

	@keyframes triquetra-rotate {
		to {
			transform: rotate(360deg);
		}
	}
</style>
