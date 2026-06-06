/**
 * Tab-completion helpers for the contenteditable input field.
 * No Svelte reactivity — safe to unit-test in jsdom.
 */

import type { KnownNoun } from '../../stores/nouns';

export interface CompletionState {
	active: boolean;
	prefix: string;
	matches: KnownNoun[];
	currentIndex: number;
	prefixStart: number;
	replacedLength: number;
}

export function resetCompletion(): CompletionState {
	return {
		active: false,
		prefix: '',
		matches: [],
		currentIndex: 0,
		prefixStart: 0,
		replacedLength: 0,
	};
}

/** Extract the word being typed from the cursor position backward. */
export function extractPrefix(): {
	prefix: string;
	start: number;
	node: Text;
} | null {
	const sel = window.getSelection();
	if (!sel || sel.rangeCount === 0) return null;

	const range = sel.getRangeAt(0);
	const node = range.startContainer;
	if (node.nodeType !== Node.TEXT_NODE) return null;

	const fullText = node.textContent ?? '';
	const cursorPos = range.startOffset;

	// Walk backward from cursor to find word start
	let start = cursorPos;
	while (
		start > 0 &&
		fullText[start - 1] !== ' ' &&
		fullText[start - 1] !== '\n' &&
		fullText[start - 1] !== ' '
	) {
		start--;
	}

	const prefix = fullText.slice(start, cursorPos);
	if (prefix.length === 0) return null;

	return { prefix, start, node: node as Text };
}

/** Replace the prefix text in the editor with the selected completion. */
export function applyCompletion(
	el: HTMLDivElement | null,
	completion: CompletionState,
): CompletionState {
	if (!el || !completion.active) return completion;

	const match = completion.matches[completion.currentIndex];
	const sel = window.getSelection();
	if (!sel || sel.rangeCount === 0) return completion;

	const range = sel.getRangeAt(0);
	const node = range.startContainer;

	// Empty editor — no text node yet, insert one
	if (node.nodeType !== Node.TEXT_NODE) {
		const textNode = document.createTextNode(match.text);
		el.textContent = '';
		el.appendChild(textNode);
		const newRange = document.createRange();
		newRange.setStart(textNode, match.text.length);
		newRange.collapse(true);
		sel.removeAllRanges();
		sel.addRange(newRange);
		return { ...completion, replacedLength: match.text.length };
	}

	const text = node.textContent ?? '';
	const replaceLen =
		completion.replacedLength > 0
			? completion.replacedLength
			: completion.prefix.length;
	const before = text.slice(0, completion.prefixStart);
	const after = text.slice(completion.prefixStart + replaceLen);

	node.textContent = before + match.text + after;

	// Place cursor after completed text
	const cursorPos = completion.prefixStart + match.text.length;
	const newRange = document.createRange();
	newRange.setStart(node, Math.min(cursorPos, node.textContent!.length));
	newRange.collapse(true);
	sel.removeAllRanges();
	sel.addRange(newRange);

	return { ...completion, replacedLength: match.text.length };
}
