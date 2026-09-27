import { describe, expect, it } from 'vitest';
import { resetCompletion, extractPrefix } from './completion';

describe('resetCompletion', () => {
	it('returns a zeroed CompletionState', () => {
		const state = resetCompletion();
		expect(state.active).toBe(false);
		expect(state.prefix).toBe('');
		expect(state.matches).toEqual([]);
		expect(state.currentIndex).toBe(0);
		expect(state.prefixStart).toBe(0);
		expect(state.replacedLength).toBe(0);
	});

	it('returns a new object each time (no aliasing)', () => {
		const a = resetCompletion();
		const b = resetCompletion();
		expect(a).not.toBe(b);
	});
});

describe('extractPrefix', () => {
	it('returns null when there is no selection', () => {
		// jsdom getSelection() returns null or a selection with rangeCount 0 by default
		const result = extractPrefix();
		expect(result).toBeNull();
	});

	it('returns null when selection is on a non-text node', () => {
		const div = document.createElement('div');
		document.body.appendChild(div);

		const sel = window.getSelection();
		if (!sel) return;

		const range = document.createRange();
		range.setStart(div, 0);
		range.collapse(true);
		sel.removeAllRanges();
		sel.addRange(range);

		expect(extractPrefix()).toBeNull();

		sel.removeAllRanges();
		document.body.removeChild(div);
	});

	it('returns the word before the cursor', () => {
		const div = document.createElement('div');
		const text = document.createTextNode('go north');
		div.appendChild(text);
		document.body.appendChild(div);

		const sel = window.getSelection();
		if (!sel) return;

		const range = document.createRange();
		// Cursor after "north" (position 8)
		range.setStart(text, 8);
		range.collapse(true);
		sel.removeAllRanges();
		sel.addRange(range);

		const result = extractPrefix();
		expect(result).not.toBeNull();
		expect(result!.prefix).toBe('north');
		expect(result!.start).toBe(3);
		expect(result!.node).toBe(text);

		sel.removeAllRanges();
		document.body.removeChild(div);
	});

	it('returns null when cursor is at a word boundary (space before)', () => {
		const div = document.createElement('div');
		const text = document.createTextNode('go ');
		div.appendChild(text);
		document.body.appendChild(div);

		const sel = window.getSelection();
		if (!sel) return;

		const range = document.createRange();
		range.setStart(text, 3); // After the space
		range.collapse(true);
		sel.removeAllRanges();
		sel.addRange(range);

		expect(extractPrefix()).toBeNull();

		sel.removeAllRanges();
		document.body.removeChild(div);
	});
});
