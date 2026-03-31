import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { streamingActive, npcsHere } from '../stores/game';
import InputField from './InputField.svelte';

// Mock ipc submitInput
vi.mock('$lib/ipc', () => ({
	submitInput: vi.fn(async (_text: string) => {})
}));

describe('InputField', () => {
	beforeEach(() => {
		streamingActive.set(false);
		npcsHere.set([]);
	});

	it('renders an editable input area', () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('textbox');
		expect(editor).toBeTruthy();
		expect(editor.getAttribute('contenteditable')).toBe('true');
	});

	it('shows placeholder when empty', () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('textbox');
		expect(editor.dataset.placeholder).toBe('What do you do? (@ to mention NPC)');
	});

	it('is not editable when streaming', () => {
		streamingActive.set(true);
		const { getByRole } = render(InputField);
		const editor = getByRole('textbox');
		expect(editor.getAttribute('contenteditable')).toBe('false');
	});

	it('clears editor after submit', async () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('textbox');
		// Type text into contenteditable
		editor.textContent = 'hello';
		await fireEvent.input(editor);
		await fireEvent.keyDown(editor, { key: 'Enter' });
		expect(editor.textContent).toBe('');
	});

	describe('NPC mention autocomplete', () => {
		const testNpcs = [
			{ name: 'Padraig Darcy', occupation: 'Publican', mood: 'content', introduced: true, mood_emoji: '😌' },
			{ name: 'Siobhan Murphy', occupation: 'Farmer', mood: 'determined', introduced: true, mood_emoji: '😤' },
			{ name: 'Father Callahan', occupation: 'Priest', mood: 'serene', introduced: false, mood_emoji: '😌' }
		];

		beforeEach(() => {
			npcsHere.set(testNpcs);
		});

		/** Helper to simulate typing into contenteditable with cursor position. */
		function typeIntoEditor(editor: HTMLElement, text: string) {
			editor.textContent = text;
			// Set cursor at end
			const range = document.createRange();
			const sel = window.getSelection();
			if (editor.firstChild) {
				range.setStart(editor.firstChild, text.length);
			} else {
				range.setStart(editor, 0);
			}
			range.collapse(true);
			sel?.removeAllRanges();
			sel?.addRange(range);
		}

		it('shows mention dropdown when @ is typed with a letter', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			expect(queryByRole('listbox')).toBeNull();

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();
		});

		it('filters NPCs by typed text', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].textContent).toContain('Padraig Darcy');
		});

		it('shows matching NPCs for @S', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@S');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].textContent).toContain('Siobhan Murphy');
		});

		it('does not show dropdown when no NPCs present', async () => {
			npcsHere.set([]);
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeNull();
		});

		it('dismisses dropdown on Escape', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();

			await fireEvent.keyDown(editor, { key: 'Escape' });
			expect(queryByRole('listbox')).toBeNull();
		});

		it('shows occupation for introduced NPCs', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options[0].textContent).toContain('Publican');
		});

		it('inserts a mention chip with full name on selection', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);

			// Select via Enter
			await fireEvent.keyDown(editor, { key: 'Enter' });

			// Should have a chip with full name
			const chip = editor.querySelector('.mention-chip');
			expect(chip).toBeTruthy();
			expect(chip?.textContent).toBe('@Padraig Darcy');
			expect((chip as HTMLElement)?.dataset.npc).toBe('Padraig Darcy');
		});
	});
});
