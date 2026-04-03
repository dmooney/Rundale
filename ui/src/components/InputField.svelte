<script lang="ts">
	import { streamingActive, npcsHere, mapData } from '../stores/game';
	import { submitInput } from '$lib/ipc';
	import { filterCommands, type SlashCommand } from '$lib/slash-commands';
	import { knownNouns, findMatches, type KnownNoun } from '../stores/nouns';
	import { get } from 'svelte/store';

	let editorEl: HTMLDivElement;

	// ── Unified dropdown state ──────────────────────────────────────────────
	type DropdownMode = 'mention' | 'slash' | null;
	let dropdownMode: DropdownMode = $state(null);
	let selectedIndex = $state(0);
	let mentionQuery = $state('');
	let slashQuery = $state('');

	const filteredNpcs = $derived(
		mentionQuery === ''
			? $npcsHere
			: $npcsHere.filter((npc) =>
					npc.name.toLowerCase().startsWith(mentionQuery.toLowerCase())
				)
	);

	const filteredCommands = $derived(filterCommands(slashQuery));

	// ── Input history ───────────────────────────────────────────────────────
	const HISTORY_KEY = 'parish-input-history';
	const HISTORY_MAX = 50;

	function loadHistory(): string[] {
		try {
			const raw = localStorage.getItem(HISTORY_KEY);
			if (raw) return JSON.parse(raw);
		} catch { /* ignore corrupt data */ }
		return [];
	}

	function saveHistory(h: string[]) {
		try { localStorage.setItem(HISTORY_KEY, JSON.stringify(h)); } catch { /* quota */ }
	}

	let history: string[] = $state(loadHistory());
	let historyIndex: number = $state(-1);
	let savedDraft: string = $state('');

	// ── Adjacent locations for quick-travel ─────────────────────────────────
	const adjacentLocations = $derived(
		($mapData?.locations ?? [])
			.filter((loc) => loc.adjacent && loc.id !== $mapData?.player_location)
			.sort((a, b) => a.name.localeCompare(b.name))
	);

	// ── Tab-completion state ────────────────────────────────────────────────
	interface CompletionState {
		active: boolean;
		prefix: string;
		matches: KnownNoun[];
		currentIndex: number;
		prefixStart: number;
		replacedLength: number;
	}

	let completion = $state<CompletionState>({
		active: false,
		prefix: '',
		matches: [],
		currentIndex: 0,
		prefixStart: 0,
		replacedLength: 0
	});

	function resetCompletion() {
		completion = {
			active: false,
			prefix: '',
			matches: [],
			currentIndex: 0,
			prefixStart: 0,
			replacedLength: 0
		};
	}

	/** Extract the word being typed from the cursor position backward. */
	function extractPrefix(): { prefix: string; start: number; node: Text } | null {
		if (!editorEl) return null;
		const sel = window.getSelection();
		if (!sel || sel.rangeCount === 0) return null;

		const range = sel.getRangeAt(0);
		const node = range.startContainer;
		if (node.nodeType !== Node.TEXT_NODE) return null;

		const fullText = node.textContent ?? '';
		const cursorPos = range.startOffset;

		// Walk backward from cursor to find word start
		let start = cursorPos;
		while (start > 0 && fullText[start - 1] !== ' ' && fullText[start - 1] !== '\n' && fullText[start - 1] !== '\u00A0') {
			start--;
		}

		const prefix = fullText.slice(start, cursorPos);
		if (prefix.length === 0) return null;

		return { prefix, start, node: node as Text };
	}

	/** Replace the prefix text in the editor with the selected completion. */
	function applyCompletion() {
		if (!editorEl || !completion.active) return;

		const match = completion.matches[completion.currentIndex];
		const sel = window.getSelection();
		if (!sel || sel.rangeCount === 0) return;

		const range = sel.getRangeAt(0);
		const node = range.startContainer;

		// Empty editor — no text node yet, insert one
		if (node.nodeType !== Node.TEXT_NODE) {
			const textNode = document.createTextNode(match.text);
			editorEl.innerHTML = '';
			editorEl.appendChild(textNode);
			completion.replacedLength = match.text.length;
			const newRange = document.createRange();
			newRange.setStart(textNode, match.text.length);
			newRange.collapse(true);
			sel.removeAllRanges();
			sel.addRange(newRange);
			return;
		}

		const text = node.textContent ?? '';
		const replaceLen = completion.replacedLength > 0 ? completion.replacedLength : completion.prefix.length;
		const before = text.slice(0, completion.prefixStart);
		const after = text.slice(completion.prefixStart + replaceLen);

		node.textContent = before + match.text + after;
		completion.replacedLength = match.text.length;

		// Place cursor after completed text
		const cursorPos = completion.prefixStart + match.text.length;
		const newRange = document.createRange();
		newRange.setStart(node, Math.min(cursorPos, node.textContent!.length));
		newRange.collapse(true);
		sel.removeAllRanges();
		sel.addRange(newRange);
	}

	// ── Focus management ────────────────────────────────────────────────────
	$effect(() => {
		if (!$streamingActive && editorEl) {
			editorEl.focus();
		}
	});

	$effect(() => {
		if (dropdownMode === 'mention' && selectedIndex >= filteredNpcs.length) {
			selectedIndex = Math.max(0, filteredNpcs.length - 1);
		}
		if (dropdownMode === 'slash' && selectedIndex >= filteredCommands.length) {
			selectedIndex = Math.max(0, filteredCommands.length - 1);
		}
	});

	// ── Editor helpers ──────────────────────────────────────────────────────

	/** Returns the full plain-text content of the editor, converting chips to @Name. */
	function getPlainText(): string {
		if (!editorEl) return '';
		return extractText(editorEl);
	}

	/** Recursively extract text from a node, handling <br> and <div> wrappers. */
	function extractText(node: Node): string {
		let result = '';
		for (const child of node.childNodes) {
			if (child.nodeType === Node.TEXT_NODE) {
				result += child.textContent ?? '';
			} else if (child instanceof HTMLElement && child.dataset.npc) {
				result += `@${child.dataset.npc}`;
			} else if (child instanceof HTMLElement && child.tagName === 'BR') {
				result += '\n';
			} else if (child instanceof HTMLElement && child.tagName === 'DIV') {
				// Chrome wraps new lines in <div> elements in contenteditable
				const inner = extractText(child);
				if (inner && result && !result.endsWith('\n')) {
					result += '\n';
				}
				result += inner;
			} else if (child instanceof HTMLElement) {
				result += child.textContent ?? '';
			}
		}
		return result.replace(/\u00A0/g, ' ');
	}

	/** Returns true if the editor is visually empty (no text, no chips). */
	function isEditorEmpty(): boolean {
		if (!editorEl) return true;
		return getPlainText().trim() === '';
	}

	/** Clears the editor content. */
	function clearEditor() {
		if (editorEl) {
			editorEl.innerHTML = '';
		}
	}

	/** Sets the editor's plain-text content and places cursor at end. */
	function setEditorText(text: string) {
		if (!editorEl) return;
		editorEl.textContent = text;
		// Place cursor at end
		const sel = window.getSelection();
		const range = document.createRange();
		range.selectNodeContents(editorEl);
		range.collapse(false);
		sel?.removeAllRanges();
		sel?.addRange(range);
	}

	/** Gets the plain text currently being typed (excluding chips). */
	function getCurrentTypingText(): string {
		if (!editorEl) return '';
		const sel = window.getSelection();
		if (sel && sel.rangeCount > 0) {
			const node = sel.getRangeAt(0).startContainer;
			if (node.nodeType === Node.TEXT_NODE) {
				return node.textContent ?? '';
			}
		}
		return getPlainText();
	}

	/** Returns true if the cursor is on the first line (or editor is empty/single-line). */
	function isCursorOnFirstLine(): boolean {
		if (!editorEl) return true;
		const text = getPlainText();
		if (!text.includes('\n')) return true;
		const sel = window.getSelection();
		if (!sel || sel.rangeCount === 0) return true;
		const range = sel.getRangeAt(0);
		// Compare cursor Y to editor top — if within first line height, we're on line 1
		const rangeRect = range.getBoundingClientRect();
		const editorRect = editorEl.getBoundingClientRect();
		// If range has no rect (empty), assume first line
		if (rangeRect.top === 0 && rangeRect.bottom === 0) return true;
		const lineHeight = parseFloat(getComputedStyle(editorEl).lineHeight) || 20;
		return rangeRect.top - editorRect.top < lineHeight;
	}

	/** Returns true if the cursor is on the last line. */
	function isCursorOnLastLine(): boolean {
		if (!editorEl) return true;
		const text = getPlainText();
		if (!text.includes('\n')) return true;
		const sel = window.getSelection();
		if (!sel || sel.rangeCount === 0) return true;
		const range = sel.getRangeAt(0);
		const rangeRect = range.getBoundingClientRect();
		const editorRect = editorEl.getBoundingClientRect();
		if (rangeRect.top === 0 && rangeRect.bottom === 0) return true;
		const lineHeight = parseFloat(getComputedStyle(editorEl).lineHeight) || 20;
		return editorRect.bottom - rangeRect.bottom < lineHeight;
	}

	// ── Mention detection ───────────────────────────────────────────────────

	function findMentionTrigger(): { query: string } | null {
		const text = getCurrentTypingText();
		const atIdx = text.lastIndexOf('@');
		if (atIdx === -1) return null;
		if (atIdx > 0 && text[atIdx - 1] !== ' ') return null;
		const query = text.slice(atIdx + 1);
		if (query.includes(' ')) return null;
		return { query };
	}

	function detectMention() {
		const trigger = findMentionTrigger();
		if (trigger !== null && $npcsHere.length > 0) {
			mentionQuery = trigger.query;
			dropdownMode = 'mention';
			selectedIndex = 0;
		} else if (dropdownMode === 'mention') {
			dropdownMode = null;
		}
	}

	// ── Slash detection ─────────────────────────────────────────────────────

	function detectSlash() {
		const text = getPlainText();
		// Slash commands must be the first character and no space yet (still typing the command)
		if (!text.startsWith('/')) {
			if (dropdownMode === 'slash') dropdownMode = null;
			return;
		}
		const spaceIdx = text.indexOf(' ');
		const query = spaceIdx === -1 ? text.slice(1) : '';
		// If user has typed a space (entering args), dismiss dropdown
		if (spaceIdx !== -1) {
			if (dropdownMode === 'slash') dropdownMode = null;
			return;
		}
		slashQuery = query;
		dropdownMode = 'slash';
		selectedIndex = 0;
	}

	// ── NPC mention chip selection ──────────────────────────────────────────

	function selectNpc(npcName: string) {
		if (!editorEl) return;

		const sel = window.getSelection();
		let textNode: Text | null = null;
		let cursorOffset = 0;

		if (sel && sel.rangeCount > 0) {
			const range = sel.getRangeAt(0);
			const node = range.startContainer;
			if (node.nodeType === Node.TEXT_NODE) {
				textNode = node as Text;
				cursorOffset = range.startOffset;
			}
		}

		if (!textNode) {
			for (const child of editorEl.childNodes) {
				if (child.nodeType === Node.TEXT_NODE && (child.textContent ?? '').includes('@')) {
					textNode = child as Text;
					cursorOffset = (child.textContent ?? '').length;
					break;
				}
			}
		}

		if (!textNode) {
			const chip = document.createElement('span');
			chip.className = 'mention-chip';
			chip.contentEditable = 'false';
			chip.dataset.npc = npcName;
			chip.textContent = `@${npcName}`;
			editorEl.innerHTML = '';
			editorEl.appendChild(chip);
			const trailing = document.createTextNode('\u00A0');
			editorEl.appendChild(trailing);
			const range = document.createRange();
			range.setStart(trailing, 1);
			range.collapse(true);
			sel?.removeAllRanges();
			sel?.addRange(range);
			dropdownMode = null;
			editorEl.focus();
			return;
		}

		const text = textNode.textContent ?? '';
		const atIdx = text.lastIndexOf('@');
		if (atIdx === -1) {
			dropdownMode = null;
			return;
		}

		const before = text.slice(0, atIdx);
		const after = text.slice(cursorOffset);

		const chip = document.createElement('span');
		chip.className = 'mention-chip';
		chip.contentEditable = 'false';
		chip.dataset.npc = npcName;
		chip.textContent = `@${npcName}`;

		const parent = textNode.parentNode!;
		if (before) {
			parent.insertBefore(document.createTextNode(before), textNode);
		}
		parent.insertBefore(chip, textNode);
		const trailing = document.createTextNode(`\u00A0${after}`);
		parent.insertBefore(trailing, textNode);
		parent.removeChild(textNode);

		const range = document.createRange();
		range.setStart(trailing, 1);
		range.collapse(true);
		sel?.removeAllRanges();
		sel?.addRange(range);

		dropdownMode = null;
		editorEl.focus();
	}

	// ── Slash command selection ──────────────────────────────────────────────

	function selectSlashCommand(cmd: SlashCommand) {
		if (cmd.hasArgs) {
			setEditorText(cmd.command + ' ');
			dropdownMode = null;
			editorEl?.focus();
		} else {
			clearEditor();
			dropdownMode = null;
			submitInput(cmd.command);
		}
	}

	/** Dissolves a mention chip back into plain text. */
	function dissolveChip(chip: HTMLElement) {
		const text = chip.textContent ?? '';
		const textNode = document.createTextNode(text);
		chip.parentNode?.replaceChild(textNode, chip);
		const sel = window.getSelection();
		const range = document.createRange();
		range.setStart(textNode, text.length);
		range.collapse(true);
		sel?.removeAllRanges();
		sel?.addRange(range);
	}

	// ── Quick-travel ────────────────────────────────────────────────────────

	async function quickTravel(locationName: string) {
		if ($streamingActive) return;
		clearEditor();
		await submitInput(`go to ${locationName}`);
	}

	// ── Submit ──────────────────────────────────────────────────────────────

	async function handleSubmit(e: Event) {
		e.preventDefault();
		// If dropdown is open, select the highlighted item
		if (dropdownMode === 'mention' && filteredNpcs.length > 0) {
			selectNpc(filteredNpcs[selectedIndex].name);
			return;
		}
		if (dropdownMode === 'slash' && filteredCommands.length > 0) {
			selectSlashCommand(filteredCommands[selectedIndex]);
			return;
		}
		const trimmed = getPlainText().trim();
		if (!trimmed || $streamingActive) return;
		clearEditor();
		dropdownMode = null;

		// Push to history (skip consecutive dupes)
		if (history.length === 0 || history[history.length - 1] !== trimmed) {
			history = [...history.slice(-(HISTORY_MAX - 1)), trimmed];
			saveHistory(history);
		}
		historyIndex = -1;

		await submitInput(trimmed);
	}

	// ── Keyboard handling ───────────────────────────────────────────────────

	function handleKeydown(e: KeyboardEvent) {
		// Dropdown navigation (mention or slash)
		if (dropdownMode !== null) {
			const items = dropdownMode === 'mention' ? filteredNpcs : filteredCommands;
			if (items.length > 0) {
				if (e.key === 'ArrowDown') {
					e.preventDefault();
					selectedIndex = (selectedIndex + 1) % items.length;
					scrollDropdownToSelected();
					return;
				}
				if (e.key === 'ArrowUp') {
					e.preventDefault();
					selectedIndex = (selectedIndex - 1 + items.length) % items.length;
					scrollDropdownToSelected();
					return;
				}
				if (e.key === 'Tab') {
					e.preventDefault();
					if (dropdownMode === 'mention') {
						selectNpc(filteredNpcs[selectedIndex].name);
					} else {
						selectSlashCommand(filteredCommands[selectedIndex]);
					}
					return;
				}
				if (e.key === 'Escape') {
					e.preventDefault();
					dropdownMode = null;
					return;
				}
			}
		}

		// Tab-completion for known nouns (only when no dropdown is open)
		if (e.key === 'Tab' && dropdownMode === null) {
			e.preventDefault();

			if (completion.active) {
				// Already cycling — advance to next match
				completion.currentIndex =
					(completion.currentIndex + 1) % completion.matches.length;
				applyCompletion();
				return;
			}

			// Start new completion
			const nouns = get(knownNouns);
			const extracted = extractPrefix();

			if (!extracted) {
				// No word typed yet — cycle through all explored locations
				const locationNouns = nouns.filter((n) => n.category === 'location');
				if (locationNouns.length === 0) return;
				const sel2 = window.getSelection();
				if (!sel2 || sel2.rangeCount === 0) return;
				const range2 = sel2.getRangeAt(0);
				const prefixStart =
					range2.startContainer.nodeType === Node.TEXT_NODE ? range2.startOffset : 0;
				completion = {
					active: true,
					prefix: '',
					matches: locationNouns,
					currentIndex: 0,
					prefixStart,
					replacedLength: 0
				};
				applyCompletion();
				return;
			}

			const matches = findMatches(extracted.prefix, nouns);
			if (matches.length === 0) return;

			completion = {
				active: true,
				prefix: extracted.prefix,
				matches,
				currentIndex: 0,
				prefixStart: extracted.start,
				replacedLength: 0
			};
			applyCompletion();
			return;
		}

		// Any other key while completing → accept and reset
		if (completion.active && e.key !== 'Shift' && e.key !== 'Tab') {
			resetCompletion();
		}

		// Input history (only when no dropdown is open)
		if (dropdownMode === null && e.key === 'ArrowUp' && isCursorOnFirstLine()) {
			if (history.length > 0) {
				e.preventDefault();
				if (historyIndex === -1) {
					savedDraft = getPlainText();
					historyIndex = history.length - 1;
				} else if (historyIndex > 0) {
					historyIndex--;
				}
				setEditorText(history[historyIndex]);
				return;
			}
		}
		if (dropdownMode === null && e.key === 'ArrowDown' && historyIndex >= 0 && isCursorOnLastLine()) {
			e.preventDefault();
			if (historyIndex < history.length - 1) {
				historyIndex++;
				setEditorText(history[historyIndex]);
			} else {
				historyIndex = -1;
				setEditorText(savedDraft);
			}
			return;
		}

		// Backspace into a chip: dissolve it
		if (e.key === 'Backspace') {
			const sel = window.getSelection();
			if (sel && sel.rangeCount > 0) {
				const range = sel.getRangeAt(0);
				if (range.collapsed && range.startOffset === 0 && range.startContainer.nodeType === Node.TEXT_NODE) {
					const prev = range.startContainer.previousSibling;
					if (prev instanceof HTMLElement && prev.dataset.npc) {
						e.preventDefault();
						dissolveChip(prev);
						return;
					}
				}
				if (range.collapsed && range.startContainer === editorEl) {
					const idx = range.startOffset;
					const child = editorEl.childNodes[idx - 1];
					if (child instanceof HTMLElement && child.dataset.npc) {
						e.preventDefault();
						dissolveChip(child);
						return;
					}
				}
			}
		}

		// Delete into a chip: dissolve it
		if (e.key === 'Delete') {
			const sel = window.getSelection();
			if (sel && sel.rangeCount > 0) {
				const range = sel.getRangeAt(0);
				if (range.collapsed) {
					const node = range.startContainer;
					if (node.nodeType === Node.TEXT_NODE && range.startOffset === (node.textContent?.length ?? 0)) {
						const next = node.nextSibling;
						if (next instanceof HTMLElement && next.dataset.npc) {
							e.preventDefault();
							dissolveChip(next);
							return;
						}
					}
				}
			}
		}

		// Enter: Shift+Enter inserts newline, plain Enter submits
		if (e.key === 'Enter') {
			if (e.shiftKey) {
				// Let the browser handle <br> insertion in contenteditable
				return;
			}
			e.preventDefault();
			handleSubmit(e);
		}
	}

	/** Scrolls the dropdown so the selected item is visible. */
	function scrollDropdownToSelected() {
		// Use tick-like defer so the DOM class has updated
		requestAnimationFrame(() => {
			const dropdown = document.querySelector('.mention-dropdown');
			const selected = dropdown?.querySelector('.mention-item.selected');
			if (selected) {
				selected.scrollIntoView({ block: 'nearest' });
			}
		});
	}

	function handleInput() {
		// Reset history browsing on any typed input
		if (historyIndex >= 0) {
			historyIndex = -1;
		}
		// Reset tab-completion on any typed input
		if (completion.active) {
			resetCompletion();
		}
		detectMention();
		if (dropdownMode !== 'mention') {
			detectSlash();
		}
	}

	// Prevent pasting rich content — only plain text
	function handlePaste(e: ClipboardEvent) {
		e.preventDefault();
		const text = e.clipboardData?.getData('text/plain') ?? '';
		document.execCommand('insertText', false, text);
	}
</script>

<div class="input-wrapper">
	{#if dropdownMode === 'mention' && filteredNpcs.length > 0}
		<ul class="mention-dropdown" role="listbox" aria-label="Mention NPC">
			{#each filteredNpcs as npc, i}
				<li
					role="option"
					aria-selected={i === selectedIndex}
					class="mention-item"
					class:selected={i === selectedIndex}
					onmousedown={(e) => { e.preventDefault(); selectNpc(npc.name); }}
					onmouseenter={() => (selectedIndex = i)}
				>
					<span class="mention-name">{npc.name}</span>
					{#if npc.introduced}
						<span class="mention-detail">{npc.occupation}</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
	{#if dropdownMode === 'slash' && filteredCommands.length > 0}
		<ul class="mention-dropdown" role="listbox" aria-label="Slash commands">
			{#each filteredCommands as cmd, i}
				<li
					role="option"
					aria-selected={i === selectedIndex}
					class="mention-item"
					class:selected={i === selectedIndex}
					onmousedown={(e) => { e.preventDefault(); selectSlashCommand(cmd); }}
					onmouseenter={() => (selectedIndex = i)}
				>
					<span class="mention-name">{cmd.command}</span>
					<span class="mention-detail">{cmd.description}</span>
				</li>
			{/each}
		</ul>
	{/if}
	{#if adjacentLocations.length > 0 && !$streamingActive}
		<div class="travel-chips">
			{#each adjacentLocations as loc}
				<button
					class="travel-chip"
					onclick={() => quickTravel(loc.name)}
					disabled={$streamingActive}
				>
					{loc.name}
				</button>
			{/each}
		</div>
	{/if}
	<div class="input-form">
		<div class="editor-wrap">
			<div
				bind:this={editorEl}
				class="input-field"
				class:disabled={$streamingActive}
				contenteditable={!$streamingActive}
				role="textbox"
				tabindex="0"
				aria-label="Player input"
				onkeydown={handleKeydown}
				oninput={handleInput}
				onpaste={handlePaste}
				data-placeholder={$streamingActive ? 'Waiting…' : 'What do you do? (@ to mention NPC)'}
			></div>
		</div>
		<button type="button" onclick={handleSubmit} disabled={$streamingActive || isEditorEmpty()} class="send-btn">
			Send
		</button>
	</div>
</div>

<style>
	.input-wrapper {
		position: relative;
		flex: 0 0 auto;
	}

	.input-form {
		display: flex;
		gap: 0.5rem;
		padding: 0.6rem 0.75rem;
		background: var(--color-panel-bg);
		border-top: 1px solid var(--color-border);
	}

	.editor-wrap {
		flex: 1;
		position: relative;
	}

	.input-field {
		background: var(--color-input-bg);
		border: 1px solid var(--color-border);
		color: var(--color-fg);
		padding: 0.5rem 0.75rem;
		font-size: 0.95rem;
		font-family: inherit;
		border-radius: 4px;
		outline: none;
		max-height: 6em;
		overflow-y: auto;
		white-space: pre-wrap;
		word-wrap: break-word;
		overflow-wrap: break-word;
	}

	.input-field:focus {
		border-color: var(--color-accent);
	}

	.input-field.disabled {
		opacity: 0.5;
		cursor: not-allowed;
		pointer-events: none;
	}

	/* Placeholder via :empty pseudo-element */
	.input-field:empty::before {
		content: attr(data-placeholder);
		color: var(--color-muted);
		pointer-events: none;
	}

	.input-field :global(.mention-chip) {
		display: inline;
		font-weight: 700;
		color: var(--color-accent);
		border: 1.5px solid var(--color-accent);
		border-radius: 3px;
		padding: 0.05em 0.3em;
		margin: 0 0.1em;
		cursor: default;
		user-select: all;
		white-space: nowrap;
	}

	.send-btn {
		background: var(--color-accent);
		color: var(--color-bg);
		border: none;
		padding: 0.5rem 1rem;
		font-size: 0.85rem;
		font-family: inherit;
		font-weight: 600;
		border-radius: 4px;
		cursor: pointer;
		transition: opacity 0.15s;
	}

	.send-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.send-btn:hover:not(:disabled) {
		opacity: 0.85;
	}

	.mention-dropdown {
		position: absolute;
		bottom: 100%;
		left: 0.75rem;
		right: 0.75rem;
		margin: 0;
		padding: 0.25rem 0;
		list-style: none;
		background: var(--color-panel-bg);
		border: 1px solid var(--color-border);
		border-radius: 4px;
		box-shadow: 0 -2px 8px rgba(0, 0, 0, 0.3);
		max-height: 12rem;
		overflow-y: auto;
		z-index: 10;
	}

	.mention-item {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0.75rem;
		cursor: pointer;
		color: var(--color-fg);
		font-size: 0.9rem;
	}

	.mention-item.selected {
		background: var(--color-accent);
		color: var(--color-bg);
	}

	.mention-name {
		font-weight: 600;
	}

	.mention-detail {
		font-size: 0.8rem;
		opacity: 0.7;
	}

	.mention-item.selected .mention-detail {
		opacity: 0.85;
	}

	/* ── Quick-travel chips ────────────────────────────────────────────────── */

	.travel-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
		padding: 0.4rem 0.75rem;
		background: var(--color-panel-bg);
		border-top: 1px solid var(--color-border);
	}

	.travel-chip {
		background: var(--color-border);
		color: var(--color-fg);
		border: none;
		border-radius: 12px;
		padding: 0.25rem 0.6rem;
		font-size: 0.8rem;
		font-family: inherit;
		cursor: pointer;
		transition: background 0.15s, color 0.15s;
	}

	.travel-chip:hover:not(:disabled) {
		background: var(--color-accent);
		color: var(--color-bg);
	}

	.travel-chip:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}
</style>
