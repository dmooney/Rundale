import { describe, expect, it } from 'vitest';
import { extractText, getPlainText } from './contenteditable';

describe('extractText', () => {
	it('returns plain text content', () => {
		const div = document.createElement('div');
		div.textContent = 'hello world';
		expect(extractText(div)).toBe('hello world');
	});

	it('converts <br> to newline', () => {
		const div = document.createElement('div');
		div.appendChild(document.createTextNode('line one'));
		div.appendChild(document.createElement('br'));
		div.appendChild(document.createTextNode('line two'));
		expect(extractText(div)).toBe('line one\nline two');
	});

	it('handles Chrome <div>-wrapped newlines', () => {
		const outer = document.createElement('div');
		outer.appendChild(document.createTextNode('first'));
		const inner = document.createElement('div');
		inner.textContent = 'second';
		outer.appendChild(inner);
		expect(extractText(outer)).toBe('first\nsecond');
	});

	it('does not double-insert newline when result already ends with one', () => {
		const outer = document.createElement('div');
		outer.appendChild(document.createElement('br'));
		const inner = document.createElement('div');
		inner.textContent = 'after';
		outer.appendChild(inner);
		expect(extractText(outer)).toBe('\nafter');
	});

	it('converts chip elements (data-npc) to @Name', () => {
		const div = document.createElement('div');
		div.appendChild(document.createTextNode('hello '));
		const chip = document.createElement('span');
		chip.dataset.npc = 'Brigid';
		chip.textContent = '@Brigid';
		div.appendChild(chip);
		expect(extractText(div)).toBe('hello @Brigid');
	});

	it('replaces non-breaking spaces with regular spaces', () => {
		const div = document.createElement('div');
		div.textContent = 'a b';
		expect(extractText(div)).toBe('a b');
	});
});

describe('getPlainText', () => {
	it('returns empty string for null element', () => {
		expect(getPlainText(null)).toBe('');
	});

	it('returns text content of element', () => {
		const div = document.createElement('div');
		div.textContent = 'test input';
		expect(getPlainText(div)).toBe('test input');
	});
});
