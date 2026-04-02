import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { streamingActive, npcsHere, mapData } from '../stores/game';
import InputField from './InputField.svelte';

// Mock ipc submitInput
const mockSubmitInput = vi.fn(async (_text: string) => {});
vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args)
}));

describe('InputField', () => {
	beforeEach(() => {
		streamingActive.set(false);
		npcsHere.set([]);
		mapData.set(null);
		mockSubmitInput.mockClear();
		localStorage.clear();
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
		editor.textContent = 'hello';
		await fireEvent.input(editor);
		await fireEvent.keyDown(editor, { key: 'Enter' });
		expect(editor.textContent).toBe('');
	});

	// ── NPC mention autocomplete ────────────────────────────────────────

	describe('NPC mention autocomplete', () => {
		const testNpcs = [
			{ name: 'Padraig Darcy', occupation: 'Publican', mood: 'content', introduced: true, mood_emoji: '😌' },
			{ name: 'Siobhan Murphy', occupation: 'Farmer', mood: 'determined', introduced: true, mood_emoji: '😤' },
			{ name: 'Father Callahan', occupation: 'Priest', mood: 'serene', introduced: false, mood_emoji: '😌' }
		];

		beforeEach(() => {
			npcsHere.set(testNpcs);
		});

		function typeIntoEditor(editor: HTMLElement, text: string) {
			editor.textContent = text;
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

	// ── Slash command autocomplete ──────────────────────────────────────

	describe('slash command autocomplete', () => {
		function typeIntoEditor(editor: HTMLElement, text: string) {
			editor.textContent = text;
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

		it('shows slash dropdown when / is typed', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '/');
			await fireEvent.input(editor);
			const listbox = queryByRole('listbox');
			expect(listbox).toBeTruthy();
			expect(listbox?.getAttribute('aria-label')).toBe('Slash commands');
		});

		it('filters commands by typed text', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '/pa');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].textContent).toContain('/pause');
		});

		it('navigates dropdown with ArrowDown/ArrowUp', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '/he');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].getAttribute('aria-selected')).toBe('true');
		});

		it('dismisses slash dropdown on Escape', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '/');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();

			await fireEvent.keyDown(editor, { key: 'Escape' });
			expect(queryByRole('listbox')).toBeNull();
		});

		it('selects no-arg command via Enter and submits', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '/pa');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledWith('/pause');
		});

		it('selects arg command via Tab and inserts with trailing space', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '/fo');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(queryByRole('listbox')).toBeNull();
			expect(editor.textContent).toContain('/fork ');
			expect(mockSubmitInput).not.toHaveBeenCalled();
		});

		it('does not show slash dropdown when / is mid-text', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'go to a/b');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeNull();
		});
	});

	// ── Input history ───────────────────────────────────────────────────

	describe('input history', () => {
		function typeIntoEditor(editor: HTMLElement, text: string) {
			editor.textContent = text;
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

		it('ArrowUp on empty editor with no history does nothing', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');
			await fireEvent.keyDown(editor, { key: 'ArrowUp' });
			expect(editor.textContent).toBe('');
		});

		it('recalls previous input with ArrowUp after submit', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'hello');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			typeIntoEditor(editor, 'world');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			// ArrowUp should show 'world'
			await fireEvent.keyDown(editor, { key: 'ArrowUp' });
			expect(editor.textContent).toBe('world');

			// Another ArrowUp should show 'hello'
			await fireEvent.keyDown(editor, { key: 'ArrowUp' });
			expect(editor.textContent).toBe('hello');
		});

		it('ArrowDown restores draft after navigating history', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'first');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			// Type a draft, then ArrowUp
			typeIntoEditor(editor, 'my draft');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'ArrowUp' });
			expect(editor.textContent).toBe('first');

			// ArrowDown should restore draft
			await fireEvent.keyDown(editor, { key: 'ArrowDown' });
			expect(editor.textContent).toBe('my draft');
		});

		it('persists history to localStorage', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'persist me');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			const stored = JSON.parse(localStorage.getItem('parish-input-history') ?? '[]');
			expect(stored).toContain('persist me');
		});

		it('does not store consecutive duplicate inputs', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'same');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			typeIntoEditor(editor, 'same');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			const stored = JSON.parse(localStorage.getItem('parish-input-history') ?? '[]');
			expect(stored.filter((s: string) => s === 'same').length).toBe(1);
		});
	});

	// ── Multi-line input ────────────────────────────────────────────────

	describe('multi-line input', () => {
		it('Shift+Enter does not submit', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			editor.textContent = 'line one';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter', shiftKey: true });

			// Editor should NOT be cleared (submit clears the editor)
			expect(editor.textContent).toBe('line one');
			expect(mockSubmitInput).not.toHaveBeenCalled();
		});

		it('Enter without Shift submits', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			editor.textContent = 'submit me';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledWith('submit me');
		});
	});

	// ── Location quick-travel chips ─────────────────────────────────────

	describe('quick-travel chips', () => {
		const testMapData = {
			locations: [
				{ id: 'crossroads', name: 'The Crossroads', lat: 0, lon: 0, adjacent: false },
				{ id: 'pub', name: "Darcy's Pub", lat: 0.1, lon: 0.1, adjacent: true },
				{ id: 'church', name: 'The Church', lat: 0.2, lon: 0.2, adjacent: true }
			],
			edges: [['crossroads', 'pub'], ['crossroads', 'church']] as [string, string][],
			player_location: 'crossroads'
		};

		it('renders chips for adjacent locations', () => {
			mapData.set(testMapData);
			const { container } = render(InputField);
			const chips = container.querySelectorAll('.travel-chip');
			expect(chips.length).toBe(2);
			expect(chips[0].textContent).toContain("Darcy's Pub");
			expect(chips[1].textContent).toContain('The Church');
		});

		it('does not show chips when mapData is null', () => {
			mapData.set(null);
			const { container } = render(InputField);
			expect(container.querySelector('.travel-chips')).toBeFalsy();
		});

		it('does not show current location as a chip', () => {
			mapData.set(testMapData);
			const { container } = render(InputField);
			const chipTexts = Array.from(container.querySelectorAll('.travel-chip')).map(el => el.textContent);
			expect(chipTexts).not.toContain('The Crossroads');
		});

		it('clicking a chip submits movement command', async () => {
			mapData.set(testMapData);
			const { container } = render(InputField);
			const chip = container.querySelector('.travel-chip') as HTMLButtonElement;
			await fireEvent.click(chip);
			expect(mockSubmitInput).toHaveBeenCalledWith("go to Darcy's Pub");
		});

		it('hides chips during streaming', () => {
			mapData.set(testMapData);
			streamingActive.set(true);
			const { container } = render(InputField);
			expect(container.querySelector('.travel-chips')).toBeFalsy();
		});
	});
});
