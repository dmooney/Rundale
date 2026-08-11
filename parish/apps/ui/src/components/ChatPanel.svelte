<script lang="ts">
	import { onMount, tick, untrack } from 'svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { textLog, streamingActive, loadingPhrase, loadingColor, addReaction, removeReaction, messageHints, worldState, nameHints, pushErrorLog, formatIpcError, playerSubmittedCount } from '../stores/game';
	import type { TextLogEntry } from '$lib/types';
	import { REACTION_PALETTE } from '$lib/reactions';
	import { reactToMessage } from '$lib/ipc';
	import { segmentText } from '$lib/rich-text';

	let logEl: HTMLDivElement;
	let hoveredMessageId: string | null = $state(null);
	const pendingReactions = new SvelteSet<string>();

	// Sticky-bottom flag: true when the panel is at (or near) the bottom.
	// Default true so the initial load scrolls into view.
	// Updated by the scroll listener below — the ONLY place user scroll intent
	// is read (after content mutation, not during it).
	let stickToBottom = $state(true);
	let userScrollRevision = 0;
	let userScrollIntentPending = false;

	// Track the last playerSubmittedCount value we handled so we can detect a
	// fresh increment. Initialised to the store's current value so that
	// pre-existing submissions at component mount don't trigger a spurious
	// force-scroll (#1431 item 4).
	let lastSubmittedCount = $playerSubmittedCount;
	// A streamed reply replaces its existing textLog entry for every chunk, so
	// array length is not a sufficient transcript-revision signal (#1835).
	let lastEntries = $textLog;
	// Track the last textLog length so we can detect when the player's echo
	// has actually landed in the log after a submit.
	let lastLogLength = $textLog.length;
	// Set when the player submits; cleared once we force-scroll after the
	// player's echo entry arrives (log grows while this flag is set). This
	// handles the case where the count increments BEFORE the echo text-log
	// event fires — without the flag the delta between the two effect runs
	// can exceed the near-bottom threshold and the panel stops short (#1431).
	let scrollOnNextLogGrowth = false;

	/** Align the transcript after Svelte has committed the pending DOM update.
	 *  Re-check both sticky state and user intent after tick(): a wheel/touch
	 *  scroll that happens while this request is queued must win. */
	function requestBottomFollow(force = false) {
		const requestedAtUserRevision = userScrollRevision;
		void tick().then(() => {
			if (!logEl || requestedAtUserRevision !== userScrollRevision) return;
			if (!force && !untrack(() => stickToBottom)) return;

			logEl.scrollTop = logEl.scrollHeight;
		});
	}

	function markUserScrollIntent() {
		userScrollIntentPending = true;
	}

	function handleScrollPointerDown(event: PointerEvent) {
		if (!logEl) return;
		const rect = logEl.getBoundingClientRect();
		if (event.clientX >= rect.right - 16) markUserScrollIntent();
	}

	function handleScrollKeydown(event: KeyboardEvent) {
		if (!['ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End', ' '].includes(event.key)) return;
		markUserScrollIntent();
		// If the key affected a focused child instead of scrolling its ancestor,
		// do not let stale intent classify a later layout scroll as user-driven.
		setTimeout(() => {
			userScrollIntentPending = false;
		}, 0);
	}

	/** Called on every real user scroll event. Measures whether the panel is
	 *  near the bottom and updates stickToBottom accordingly. We read geometry
	 *  here (on actual scroll) rather than after content mutations so the
	 *  measurement is never contaminated by newly-rendered content. */
	function handleScroll(event: Event) {
		if (!logEl) return;
		const nearBottom =
			logEl.scrollHeight - logEl.scrollTop - logEl.clientHeight <= 50;
		// Native layout, ResizeObserver, and scrollTop assignments can all emit
		// trusted scroll events. Only wheel/touch/scrollbar intent may unstick the
		// reader. Synthetic events remain explicit intent for deterministic tests.
		const isUserScroll = userScrollIntentPending || !event.isTrusted;
		userScrollIntentPending = false;
		if (nearBottom) {
			stickToBottom = true;
		} else if (isUserScroll) {
			stickToBottom = false;
			userScrollRevision += 1;
			// A user scroll after submit supersedes the delayed echo follow.
			scrollOnNextLogGrowth = false;
		} else if (stickToBottom) {
			// Content/layout moved the bottom before our queued follow completed.
			requestBottomFollow();
		}
	}

	onMount(() => {
		requestBottomFollow();
		if (typeof ResizeObserver === 'undefined') return;

		// The flex child changes height when the viewport, virtual keyboard, chip
		// rows, or expanding composer changes the available chat area. Preserve
		// the bottom only for readers who have not deliberately scrolled up.
		const observer = new ResizeObserver(() => {
			requestBottomFollow();
		});
		observer.observe(logEl);
		return () => observer.disconnect();
	});

	$effect(() => {
		const entries = $textLog;
		// Re-read the counter inside the effect so Svelte tracks it as a
		// reactive dependency and re-runs on every increment.
		const currentCount = $playerSubmittedCount;
		const countIncremented = currentCount > lastSubmittedCount;
		const textLogRevised = entries !== lastEntries;
		const logGrew = entries.length > lastLogLength;

		lastSubmittedCount = currentCount;
		lastEntries = entries;
		lastLogLength = entries.length;

		// Player submit: arm the one-shot flag and re-stick so the player
		// always follows their own message.
		if (countIncremented) {
			scrollOnNextLogGrowth = true;
			stickToBottom = true;
		}

		// Force-scroll when: (a) the count just incremented, OR (b) the log
		// grew while the one-shot flag was armed (the player's echo arrived in
		// a separate effect run after the count increment).  Disarm once used.
		const forceScroll = countIncremented || (scrollOnNextLogGrowth && logGrew);
		if (logGrew && scrollOnNextLogGrowth) scrollOnNextLogGrowth = false;

		// Follow every transcript revision while sticky. Streaming, finalisation,
		// correction, and reactions replace entries without growing the array.
		// Player submit remains the sole unconditional force-follow signal.
		// Read stickToBottom via untrack so scroll events don't re-trigger
		// this effect — only $textLog / $playerSubmittedCount changes should.
		const shouldScroll = forceScroll || (textLogRevised && untrack(() => stickToBottom));
		if (shouldScroll) requestBottomFollow(forceScroll);
	});

	function entryType(entry: TextLogEntry): 'player' | 'npc' | 'system' | 'command' {
		if (entry.source === 'player' && entry.subtype === 'command') return 'command';
		if (entry.source === 'player') return 'player';
		if (entry.source === 'system') return 'system';
		if (entry.subtype === 'location') return 'system';
		// Non-verbal NPC reactions (subtype "action") are rendered as italicised
		// narration in the system-message style, not as speech bubbles (#1431 item 2).
		if (entry.subtype === 'action') return 'system';
		return 'npc';
	}

	function displayLabel(entry: TextLogEntry): string {
		if (entry.source === 'player') return 'You';
		return entry.source;
	}

	type TabularRow = { kind: 'header'; text: string } | { kind: 'pair'; cmd: string; desc: string };

	/** Parses a `subtype: "tabular"` message body into header / (cmd, desc) rows.
	 *  Lines containing " — " are split into two cells; other lines become headers
	 *  spanning both columns. This lets a CSS grid handle alignment, so descriptions
	 *  line up in the proportional chat font without needing monospace. */
	function parseTabularRows(content: string): TabularRow[] {
		return content.split('\n').map((line): TabularRow => {
			const idx = line.indexOf(' — ');
			if (idx === -1) return { kind: 'header', text: line };
			return {
				kind: 'pair',
				cmd: line.slice(0, idx).trim(),
				desc: line.slice(idx + ' — '.length).trim()
			};
		});
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
		const key = `${entry.id}:${emoji}`;
		if (pendingReactions.has(key)) return;
		pendingReactions.add(key);
		// Optimistic UI update
		addReaction(entry.id, emoji, 'player');
		// Send to backend; roll back the optimistic reaction on failure
		// (#353) so the UI never shows a "saved" state that the server
		// never received. Swallowing the error caused persistent data
		// loss on reload/branch-switch because the reaction never
		// reached a snapshot.
		const messageId = entry.id;
		const snippet = entry.content.slice(0, 80);
		reactToMessage(entry.source, snippet, emoji)
			.catch((err) => {
				console.warn('reactToMessage failed:', err);
				removeReaction(messageId, emoji, 'player');
				pushErrorLog(`Could not record reaction ${emoji}: ${formatIpcError(err)}`);
			})
			.finally(() => {
				pendingReactions.delete(key);
			});
		// Close picker
		hoveredMessageId = null;
	}
</script>

<!-- The log itself is the scrollable interaction surface; these handlers only track user scroll intent. -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
	class="chat-panel"
	data-testid="chat-panel"
	bind:this={logEl}
	role="log"
	aria-live="polite"
	aria-label="Game chat log"
	onwheel={markUserScrollIntent}
	ontouchmove={markUserScrollIntent}
	onpointerdown={handleScrollPointerDown}
	onkeydown={handleScrollKeydown}
	onscroll={handleScroll}
>
	{#each $textLog as entry, index (entry.id || entry.stream_turn_id || `${entry.source}:${index}`)}
		{#if entryType(entry) === 'command'}
			<div class="entry command" data-testid="command-entry" role="log">
				<span class="command-prompt" aria-hidden="true">&gt;</span>
				<span class="command-text">{entry.content}</span>
			</div>
		{:else if entryType(entry) === 'system'}
			{@const isSplash = entry.content.includes('Copyright \u00A9')}
			{@const lines = entry.content.split('\n')}
			<div class="entry system" class:location={entry.subtype === 'location'} class:error={entry.subtype === 'error'} class:tabular={entry.subtype === 'tabular'}>
				{#if entry.subtype === 'time-rule'}
					<div class="time-rule" role="separator" aria-label={entry.content}>
						<span class="time-rule-text">{entry.content}</span>
					</div>
				{:else if entry.subtype === 'tabular'}
					{@const rows = parseTabularRows(entry.content)}
					<div class="tabular-grid">
						{#each rows as row, ri (ri)}
							{#if row.kind === 'header'}
								<div class="tabular-header">{row.text}</div>
							{:else}
								<div class="tabular-cmd">{row.cmd}</div>
								<div class="tabular-desc">— {row.desc}</div>
							{/if}
						{/each}
					</div>
				{:else if isSplash}
					<div class="splash-card">
						<strong>{lines[0]}</strong>
						<span class="splash-meta">{lines.slice(1).join('\n')}</span>
					</div>
				{:else}
					<span class="content">{#each parseEmotes(entry.content) as seg, si (si)}{#if seg.isAction}<span class="emote">{seg.text}</span>{:else}{#each richify(seg.text) as rs, rsi (rsi)}<span class="term-{rs.kind}">{rs.text}</span>{/each}{/if}{/each}</span>
				{/if}
			</div>
		{:else}
			{@const npcReactable = entryType(entry) === 'npc' && !entry.streaming && !!entry.id}
			<div class="bubble-row {entryType(entry)}">
				<div class="bubble-wrapper">
					<span class="label">{displayLabel(entry)}</span>
					<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
					<div
						class="bubble-anchor"
						class:focusable={npcReactable}
						role={npcReactable ? 'group' : undefined}
						aria-label={npcReactable ? 'NPC message — press Enter or Tab into the reaction picker' : undefined}
						tabindex={npcReactable ? 0 : -1}
						onmouseenter={() => { if (npcReactable) hoveredMessageId = entry.id ?? null; }}
						onmouseleave={() => { hoveredMessageId = null; }}
						onfocusin={() => { if (npcReactable) hoveredMessageId = entry.id ?? null; }}
						onfocusout={(e) => {
							// Only close when focus actually leaves the bubble + picker
							// subtree. Without this, tabbing from the bubble into a
							// reaction button fires focusout on the bubble before
							// focusin on the button — and we'd close the picker
							// before the user could activate it.
							const next = (e as FocusEvent).relatedTarget as Node | null;
							if (!next || !(e.currentTarget as HTMLElement).contains(next)) {
								hoveredMessageId = null;
							}
						}}
						onkeydown={(e) => {
							// Esc closes the picker (mouse users have onmouseleave).
							if (e.key === 'Escape' && npcReactable) {
								hoveredMessageId = null;
								(e.currentTarget as HTMLElement).focus();
							}
						}}
					>
						<div class="bubble">
							<span class="content"
								>{#each renderSegments(entry) as seg, si (si)}{#if seg.animate}{#key seg.animationKey}<span class="stream-chunk" class:emote={seg.isAction}>{seg.text}</span>{/key}{:else if seg.isAction}<span class="emote">{seg.text}</span>{:else}{#each richify(seg.text, entry.id) as rs, rsi (rsi)}<span class="term-{rs.kind}">{rs.text}</span>{/each}{/if}{/each}</span>
						</div>

						<!-- Reaction picker (floats over bubble, NPC messages only) -->
						{#if hoveredMessageId && hoveredMessageId === entry.id && entryType(entry) === 'npc'}
							<div class="reaction-picker" role="toolbar" aria-label="React to message" data-testid="reaction-picker">
								{#each REACTION_PALETTE as reaction (reaction.emoji)}
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
							{#each entry.reactions as r (r.emoji + r.source)}
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
	{#if $streamingActive && ($textLog.length === 0 || !$textLog[$textLog.length - 1].streaming || $textLog[$textLog.length - 1].content === '')}
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
		gap: 0.6rem;
		background: var(--color-bg);
	}

	/* Pin sparse content to the bottom without `justify-content: flex-end`,
	   which makes the overflowed top of a long log unreachable by scroll in
	   a flex scroll container. */
	.chat-panel > :global(:first-child) {
		margin-top: auto;
	}

	/* System messages: narrative prose */
	/* Command echo — player-typed slash commands shown as a distinct input line,
	   not a dialogue bubble. Monospace prompt + command text, muted so the
	   narration that follows draws the eye. */
	.entry.command {
		display: flex;
		align-items: baseline;
		gap: 0.35rem;
		padding: 0.25rem 0;
		font-family: var(--font-mono, monospace);
		font-size: 0.9rem;
		color: var(--color-muted);
		opacity: 0.8;
	}

	.command-prompt {
		color: var(--color-accent);
		font-weight: 600;
		user-select: none;
	}

	.command-text {
		letter-spacing: 0.01em;
	}

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

	/* Error feedback: subtle red left border so failures are visible
	   instead of silent — used by pushErrorLog for failed IPC calls. */
	.entry.system.error {
		border-left: 3px solid #c0554a;
		padding-left: 0.75rem;
		color: var(--color-muted);
		font-size: 0.95rem;
	}

	/* Tabular system output (e.g. /help): a two-column grid so commands
	   and descriptions line up regardless of font metrics. Keeps the
	   chat's proportional serif instead of switching to monospace. */
	.entry.system.tabular .tabular-grid {
		display: grid;
		grid-template-columns: max-content 1fr;
		column-gap: 0.75em;
		row-gap: 0;
	}
	.entry.system.tabular .tabular-header {
		grid-column: 1 / -1;
	}

	/* Inline term highlighting */
	:global(.term-irish)    { color: var(--color-irish); }
	:global(.term-name)     { color: var(--color-name); }
	:global(.term-location) { color: var(--color-location); font-style: italic; }

	/* #1226 — the player bubble is gold (var(--color-accent)) with cream text.
	   The page-tuned term colours are legible on the light NPC/system
	   background but clash on gold — the location colour (#b58900) is itself
	   gold and vanishes, hiding the go-to destination ("go to The Crossroads"
	   rendered the destination as gold-on-gold). The player echoes their own
	   command, so semantic name/location/Irish colouring adds no value here:
	   force every term span inside the player bubble back to the bubble's own
	   readable foreground. (Location keeps its italic for a subtle accent.) */
	.player .bubble :global(.term-irish),
	.player .bubble :global(.term-name),
	.player .bubble :global(.term-location) {
		color: var(--color-bg);
	}

	/* Title card: centred frontispiece for the splash message. The title
	   leads in the display face; copyright/branch metadata is demoted to
	   small muted text so it no longer opens the narrative at body size. */
	.splash-card {
		display: block;
		text-align: center;
		padding: 1.25rem 1rem 0.75rem;
	}

	.splash-card strong {
		font-family: var(--font-display);
		font-size: 1.45rem;
		letter-spacing: 0.1em;
		display: block;
		color: var(--color-accent);
		font-weight: 600;
		margin-bottom: 0.5rem;
	}

	.splash-meta {
		display: block;
		font-size: 0.78rem;
		line-height: 1.5;
		color: var(--color-muted);
		white-space: pre-wrap;
	}

	/* Time-of-day separator — small-caps label between hairline rules so
	   the 36× clock's passage is visible in the chronicle. */
	.time-rule {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.35rem 0;
	}

	.time-rule::before,
	.time-rule::after {
		content: '';
		flex: 1;
		border-top: 1px solid var(--color-border);
	}

	.time-rule-text {
		font-family: var(--font-display);
		font-size: 0.64rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		color: var(--color-muted);
		white-space: nowrap;
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

	/* #1275 — align the wrapper's stacked children (label, bubble, reaction
	   bar) to the message side. Previously children defaulted to `stretch`, so
	   when several NPC reaction chips made the bar the widest child the wrapper
	   grew to the bar's width and the narrow player bubble floated at the LEFT
	   while the chips sat elsewhere — they no longer lined up. Anchoring the
	   column to the message side keeps bubble and chips on the same edge. */
	.player .bubble-wrapper {
		align-items: flex-end;
	}

	.npc .bubble-wrapper {
		align-items: flex-start;
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

	/* Player message: italic, no rounded top-right. The bubble darkens the
	   accent toward the foreground ink so the bg-coloured text passes WCAG
	   AA (cream-on-raw-gold was ~2.3:1); fg/bg are the theme's guaranteed
	   contrast pair, so mixing toward fg raises contrast in every palette. */
	.player .bubble {
		background: color-mix(in srgb, var(--color-accent) 55%, var(--color-fg));
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

	/* Keyboard-only users get a visible focus ring on the NPC bubbles
	 * so they can find their position when tabbing. The default browser
	 * outline lands on the inner div which has rounded corners, so set
	 * outline-offset slightly negative to wrap the bubble cleanly. (#352) */
	.bubble-anchor.focusable:focus-visible {
		outline: 2px solid var(--color-accent);
		outline-offset: 2px;
		border-radius: 4px;
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

	.reaction-btn:hover,
	.reaction-btn:focus-visible {
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

	/* #1275 — NPC reactions to the player's own message attach named chips to
	   the player bubble. The bubble is right-aligned inside the 75 %-wide
	   wrapper, but the reaction bar defaulted to flex-start, so multiple chips
	   began at the wrapper's left edge — detached from the bubble and spilling
	   left. Align the bar to the message side so chips sit under their bubble:
	   right under player bubbles, left under NPC bubbles. flex-wrap keeps every
	   chip visible; chips wrap as whole units (see .reaction-badge below). */
	.player .reaction-bar {
		justify-content: flex-end;
	}

	.npc .reaction-bar {
		justify-content: flex-start;
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
		/* #1275 — keep each chip an intact, non-shrinking unit so a long NPC
		   name never wraps its emoji onto a separate line and the bar wraps by
		   whole chips, staying aligned. */
		flex: 0 0 auto;
		white-space: nowrap;
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
