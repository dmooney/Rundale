/**
 * Pure DOM helpers for the contenteditable input field.
 * No Svelte reactivity — safe to unit-test in jsdom.
 */

/** Recursively extract text from a node, handling <br> and <div> wrappers. */
export function extractText(node: Node): string {
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

/** Returns the full plain-text content of the editor, converting chips to @Name. */
export function getPlainText(el: HTMLDivElement | null): string {
	if (!el) return '';
	return extractText(el);
}

/** Clears the editor content. Does not touch Svelte reactive state. */
export function clearEditor(el: HTMLDivElement | null): void {
	if (el) {
		el.textContent = '';
	}
}

/** Sets the editor's plain-text content and places cursor at end. */
export function setEditorText(el: HTMLDivElement | null, text: string): void {
	if (!el) return;
	el.textContent = text;
	// Place cursor at end
	const sel = window.getSelection();
	const range = document.createRange();
	range.selectNodeContents(el);
	range.collapse(false);
	sel?.removeAllRanges();
	sel?.addRange(range);
}

/** Returns true if the cursor is on the first line (or editor is empty/single-line). */
export function isCursorOnFirstLine(
	el: HTMLDivElement | null,
	plainText: string,
): boolean {
	if (!el) return true;
	if (!plainText.includes('\n')) return true;
	const sel = window.getSelection();
	if (!sel || sel.rangeCount === 0) return true;
	const range = sel.getRangeAt(0);
	// Compare cursor Y to editor top — if within first line height, we're on line 1
	const rangeRect = range.getBoundingClientRect();
	const editorRect = el.getBoundingClientRect();
	// If range has no rect (empty), assume first line
	if (rangeRect.top === 0 && rangeRect.bottom === 0) return true;
	const lineHeight = parseFloat(getComputedStyle(el).lineHeight) || 20;
	return rangeRect.top - editorRect.top < lineHeight;
}

/** Returns true if the cursor is on the last line. */
export function isCursorOnLastLine(
	el: HTMLDivElement | null,
	plainText: string,
): boolean {
	if (!el) return true;
	if (!plainText.includes('\n')) return true;
	const sel = window.getSelection();
	if (!sel || sel.rangeCount === 0) return true;
	const range = sel.getRangeAt(0);
	const rangeRect = range.getBoundingClientRect();
	const editorRect = el.getBoundingClientRect();
	if (rangeRect.top === 0 && rangeRect.bottom === 0) return true;
	const lineHeight = parseFloat(getComputedStyle(el).lineHeight) || 20;
	return editorRect.bottom - rangeRect.bottom < lineHeight;
}
