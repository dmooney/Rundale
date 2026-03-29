<script lang="ts">
	import { streamingActive, npcsHere } from '../stores/game';
	import { submitInput } from '$lib/ipc';

	let editorEl: HTMLDivElement;
	let showMentions = $state(false);
	let selectedIndex = $state(0);
	let mentionQuery = $state('');

	const filteredNpcs = $derived(
		mentionQuery === ''
			? $npcsHere
			: $npcsHere.filter((npc) =>
					npc.name.toLowerCase().startsWith(mentionQuery.toLowerCase())
				)
	);

	$effect(() => {
		if (!$streamingActive && editorEl) {
			editorEl.focus();
		}
	});

	$effect(() => {
		if (selectedIndex >= filteredNpcs.length) {
			selectedIndex = Math.max(0, filteredNpcs.length - 1);
		}
	});

	/** Returns the full plain-text content of the editor, converting chips to @Name. */
	function getPlainText(): string {
		if (!editorEl) return '';
		let result = '';
		for (const node of editorEl.childNodes) {
			if (node.nodeType === Node.TEXT_NODE) {
				result += node.textContent ?? '';
			} else if (node instanceof HTMLElement && node.dataset.npc) {
				result += `@${node.dataset.npc}`;
			} else if (node instanceof HTMLElement) {
				result += node.textContent ?? '';
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

	/** Gets the plain text currently being typed (excluding chips). */
	function getCurrentTypingText(): string {
		if (!editorEl) return '';
		// Get text from the text node the cursor is in, or fall back to full text
		const sel = window.getSelection();
		if (sel && sel.rangeCount > 0) {
			const node = sel.getRangeAt(0).startContainer;
			if (node.nodeType === Node.TEXT_NODE) {
				return node.textContent ?? '';
			}
		}
		// Fallback: use the full plain text of the editor
		return getPlainText();
	}

	/** Finds an @-trigger in the text currently being typed. */
	function findMentionTrigger(): { query: string } | null {
		const text = getCurrentTypingText();
		const atIdx = text.lastIndexOf('@');
		if (atIdx === -1) return null;
		// @ must be at start or preceded by a space
		if (atIdx > 0 && text[atIdx - 1] !== ' ') return null;
		const query = text.slice(atIdx + 1);
		// Don't trigger if there's a space in the query
		if (query.includes(' ')) return null;
		return { query };
	}

	function detectMention() {
		const trigger = findMentionTrigger();
		if (trigger !== null && $npcsHere.length > 0) {
			mentionQuery = trigger.query;
			showMentions = true;
			selectedIndex = 0;
		} else {
			showMentions = false;
		}
	}

	function selectNpc(npcName: string) {
		if (!editorEl) return;

		const sel = window.getSelection();
		let textNode: Text | null = null;
		let cursorOffset = 0;

		// Find the text node containing the @mention
		if (sel && sel.rangeCount > 0) {
			const range = sel.getRangeAt(0);
			const node = range.startContainer;
			if (node.nodeType === Node.TEXT_NODE) {
				textNode = node as Text;
				cursorOffset = range.startOffset;
			}
		}

		// Fallback: find the first text node in the editor
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
			// Last resort: just replace the entire editor content
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
			showMentions = false;
			editorEl.focus();
			return;
		}

		const text = textNode.textContent ?? '';
		const atIdx = text.lastIndexOf('@');
		if (atIdx === -1) {
			showMentions = false;
			return;
		}

		const before = text.slice(0, atIdx);
		const after = text.slice(cursorOffset);

		// Build chip
		const chip = document.createElement('span');
		chip.className = 'mention-chip';
		chip.contentEditable = 'false';
		chip.dataset.npc = npcName;
		chip.textContent = `@${npcName}`;

		// Replace text node with: [before] [chip] [nbsp + after]
		const parent = textNode.parentNode!;
		if (before) {
			parent.insertBefore(document.createTextNode(before), textNode);
		}
		parent.insertBefore(chip, textNode);
		const trailing = document.createTextNode(`\u00A0${after}`);
		parent.insertBefore(trailing, textNode);
		parent.removeChild(textNode);

		// Place cursor after chip
		const range = document.createRange();
		range.setStart(trailing, 1);
		range.collapse(true);
		sel?.removeAllRanges();
		sel?.addRange(range);

		showMentions = false;
		editorEl.focus();
	}

	/** Dissolves a mention chip back into plain text. */
	function dissolveChip(chip: HTMLElement) {
		const text = chip.textContent ?? '';
		const textNode = document.createTextNode(text);
		chip.parentNode?.replaceChild(textNode, chip);
		// Place cursor at end of dissolved text
		const sel = window.getSelection();
		const range = document.createRange();
		range.setStart(textNode, text.length);
		range.collapse(true);
		sel?.removeAllRanges();
		sel?.addRange(range);
	}

	async function handleSubmit(e: Event) {
		e.preventDefault();
		if (showMentions && filteredNpcs.length > 0) {
			selectNpc(filteredNpcs[selectedIndex].name);
			return;
		}
		const trimmed = getPlainText().trim();
		if (!trimmed || $streamingActive) return;
		clearEditor();
		showMentions = false;
		await submitInput(trimmed);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (showMentions && filteredNpcs.length > 0) {
			if (e.key === 'ArrowDown') {
				e.preventDefault();
				selectedIndex = (selectedIndex + 1) % filteredNpcs.length;
				return;
			}
			if (e.key === 'ArrowUp') {
				e.preventDefault();
				selectedIndex =
					(selectedIndex - 1 + filteredNpcs.length) % filteredNpcs.length;
				return;
			}
			if (e.key === 'Tab') {
				e.preventDefault();
				selectNpc(filteredNpcs[selectedIndex].name);
				return;
			}
			if (e.key === 'Escape') {
				e.preventDefault();
				showMentions = false;
				return;
			}
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
				// Also handle: cursor is right after chip with no text node between
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

		if (e.key === 'Enter') {
			e.preventDefault();
			handleSubmit(e);
		}
	}

	function handleInput() {
		detectMention();
	}

	// Prevent pasting rich content — only plain text
	function handlePaste(e: ClipboardEvent) {
		e.preventDefault();
		const text = e.clipboardData?.getData('text/plain') ?? '';
		document.execCommand('insertText', false, text);
	}
</script>

<div class="input-wrapper">
	{#if showMentions && filteredNpcs.length > 0}
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
</style>
