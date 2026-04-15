import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { streamingActive, npcsHere, mapData } from '../stores/game';
import { findMatches, type KnownNoun } from '../stores/nouns';
import InputField from './InputField.svelte';

// Mock ipc submitInput
const mockSubmitInput = vi.fn(async (..._args: unknown[]) => {});
vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args)
}));

const localStore: Record<string, string> = {};
if (typeof localStorage === 'undefined' || typeof localStorage.getItem !== 'function') {
	Object.defineProperty(globalThis, 'localStorage', {
		configurable: true,
		value: {
			getItem: (key: string) => localStore[key] ?? null,
			setItem: (key: string, value: string) => {
				localStore[key] = value;
			},
			clear: () => {
				for (const key of Object.keys(localStore)) delete localStore[key];
			}
		}
	});
}

describe('InputField', () => {
	beforeEach(() => {
		streamingActive.set(false);
		npcsHere.set([]);
		mapData.set(null);
		mockSubmitInput.mockClear();
		localStorage.clear?.();
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

	it('enables send button when editor has text', async () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('textbox');
		const sendBtn = getByRole('button', { name: 'Send' }) as HTMLButtonElement;
		expect(sendBtn.disabled).toBe(true);

		editor.textContent = 'hello';
		await fireEvent.input(editor);
		expect(sendBtn.disabled).toBe(false);
	});

	// ── NPC mention autocomplete ────────────────────────────────────────

	describe('NPC mention autocomplete', () => {
		const testNpcs = [
			{ name: 'Padraig Darcy', real_name: 'Padraig Darcy', occupation: 'Publican', mood: 'content', introduced: true, mood_emoji: '😌' },
			{ name: 'Siobhan Murphy', real_name: 'Siobhan Murphy', occupation: 'Farmer', mood: 'determined', introduced: true, mood_emoji: '😤' },
			{ name: 'Father Callahan', real_name: 'Father Callahan', occupation: 'Priest', mood: 'serene', introduced: false, mood_emoji: '😌' }
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

			expect(mockSubmitInput).toHaveBeenCalledWith('submit me', []);
		});
	});

	// ── Location quick-travel chips ─────────────────────────────────────

	describe('npc selection buttons', () => {
		const testNpcs = [
			{ name: 'Padraig Darcy', real_name: 'Padraig Darcy', occupation: 'Publican', mood: 'content', introduced: true, mood_emoji: '😌' },
			{ name: 'an older man behind the bar', real_name: 'Tomas Brennan', occupation: 'Publican', mood: 'wary', introduced: false, mood_emoji: '😐' },
			{ name: 'Siobhan Murphy', real_name: 'Siobhan Murphy', occupation: 'Farmer', mood: 'determined', introduced: true, mood_emoji: '😤' }
		];

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

		beforeEach(() => {
			npcsHere.set(testNpcs);
		});

		it('renders npc buttons and hides occupation for unintroduced npcs', () => {
			const { container, getByText } = render(InputField);
			expect(container.querySelectorAll('.npc-chip').length).toBe(3);
			expect(getByText('Padraig Darcy')).toBeTruthy();
			expect(getByText('Publican')).toBeTruthy();
			expect((container.querySelectorAll('.npc-chip')[1] as HTMLElement).textContent).not.toContain('Publican');
		});

		it('clicking an npc chip inserts an @name mention chip into the editor', async () => {
			const { container, getByRole } = render(InputField);
			const editor = getByRole('textbox');
			const chip = container.querySelector('.npc-chip') as HTMLButtonElement;
			await fireEvent.click(chip);

			const mention = editor.querySelector('.mention-chip');
			expect(mention).toBeTruthy();
			expect(mention?.textContent).toBe('@Padraig Darcy');
			expect((mention as HTMLElement)?.dataset.npc).toBe('Padraig Darcy');
		});

		it('hides npc buttons during streaming', () => {
			streamingActive.set(true);
			const { container } = render(InputField);
			expect(container.querySelector('.npc-chips')).toBeFalsy();
		});
	});

	describe('quick-travel chips', () => {
		const testMapData = {
			locations: [
				{ id: 'crossroads', name: 'The Crossroads', lat: 0, lon: 0, adjacent: false, hops: 0 },
				{ id: 'pub', name: "Darcy's Pub", lat: 0.1, lon: 0.1, adjacent: true, hops: 1 },
				{ id: 'church', name: 'The Church', lat: 0.2, lon: 0.2, adjacent: true, hops: 1 }
			],
			edges: [['crossroads', 'pub'], ['crossroads', 'church']] as [string, string][],
			player_location: 'crossroads',
			player_lat: 0,
			player_lon: 0
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

	// ── findMatches utility ─────────────────────────────────────────────

	describe('findMatches', () => {
		const testNouns: KnownNoun[] = [
			{ text: "Darcy's Pub", category: 'location', priority: 0 },
			{ text: 'The Crossroads', category: 'location', priority: 0 },
			{ text: 'The Church', category: 'location', priority: 2 },
			{ text: 'Padraig Darcy', category: 'npc', priority: 1 },
			{ text: 'Siobhan Murphy', category: 'npc', priority: 1 }
		];

		it('matches start of any word in the noun', () => {
			const matches = findMatches('pub', testNouns);
			expect(matches.length).toBe(1);
			expect(matches[0].text).toBe("Darcy's Pub");
		});

		it('matches NPC name prefix', () => {
			const matches = findMatches('padr', testNouns);
			expect(matches.length).toBe(1);
			expect(matches[0].text).toBe('Padraig Darcy');
		});

		it('matches start of full noun text', () => {
			const matches = findMatches('the', testNouns);
			expect(matches.length).toBe(2);
			expect(matches.map((m) => m.text)).toContain('The Crossroads');
			expect(matches.map((m) => m.text)).toContain('The Church');
		});

		it('returns empty for no matches', () => {
			expect(findMatches('xyz', testNouns)).toEqual([]);
		});

		it('returns empty for empty prefix', () => {
			expect(findMatches('', testNouns)).toEqual([]);
		});

		it('is case-insensitive', () => {
			const matches = findMatches('PUB', testNouns);
			expect(matches.length).toBe(1);
			expect(matches[0].text).toBe("Darcy's Pub");
		});

		it('matches word after apostrophe', () => {
			const matches = findMatches('darcy', testNouns);
			expect(matches.length).toBe(2);
			expect(matches.map((m) => m.text)).toContain("Darcy's Pub");
			expect(matches.map((m) => m.text)).toContain('Padraig Darcy');
		});
	});

	// ── Tab-completion ──────────────────────────────────────────────────

	describe('tab-completion', () => {
		const testMapData = {
			locations: [
				{ id: 'crossroads', name: 'The Crossroads', lat: 0, lon: 0, adjacent: true, hops: 1, visited: true },
				{ id: 'pub', name: "Darcy's Pub", lat: 0.1, lon: 0.1, adjacent: true, hops: 1, visited: true },
				{ id: 'church', name: 'The Church', lat: 0.2, lon: 0.2, adjacent: false, hops: 2, visited: true },
				{ id: 'mill', name: 'The Mill', lat: 0.3, lon: 0.3, adjacent: false, hops: 3, visited: false }
			],
			edges: [
				['crossroads', 'pub'],
				['crossroads', 'church']
			] as [string, string][],
			player_location: 'crossroads',
			player_lat: 0,
			player_lon: 0
		};

		const testNpcs = [
			{
				name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				occupation: 'Publican',
				mood: 'content',
				introduced: true,
				mood_emoji: '😌'
			}
		];

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

		beforeEach(() => {
			mapData.set(testMapData);
			npcsHere.set(testNpcs);
		});

		it('Tab completes a matching prefix', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'go to pub');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(editor.textContent).toContain("Darcy's Pub");
		});

		it('Tab does nothing when no matches', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'go to xyz');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(editor.textContent).toBe('go to xyz');
		});

		it('Tab cycles through all visited locations on empty input', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			await fireEvent.keyDown(editor, { key: 'Tab' });
			const first = editor.textContent ?? '';
			expect(first.length).toBeGreaterThan(0);

			await fireEvent.keyDown(editor, { key: 'Tab' });
			const second = editor.textContent ?? '';
			expect(second).not.toBe(first);
		});

		it('Tab completes unvisited (frontier) locations with lower priority', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'mill');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			// "The Mill" is unvisited but visible (frontier) — should complete
			expect(editor.textContent).toContain('The Mill');
		});

		it('Tab cycles through multiple matches', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			// "the" matches "The Crossroads" and "The Church"
			typeIntoEditor(editor, 'the');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			const firstMatch = editor.textContent;
			expect(firstMatch === 'The Crossroads' || firstMatch === 'The Church').toBe(true);

			await fireEvent.keyDown(editor, { key: 'Tab' });
			const secondMatch = editor.textContent;
			expect(secondMatch).not.toBe(firstMatch);
			expect(secondMatch === 'The Crossroads' || secondMatch === 'The Church').toBe(true);
		});

		it('typing resets completion state', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, 'pub');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });
			expect(editor.textContent).toContain("Darcy's Pub");

			// Type something — should accept completion and reset
			await fireEvent.input(editor);

			// Now a new Tab should start fresh
			typeIntoEditor(editor, 'cross');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });
			expect(editor.textContent).toContain('The Crossroads');
		});

		it('mention dropdown Tab takes priority over noun completion', async () => {
			npcsHere.set([
				{
					name: 'Padraig Darcy',
					real_name: 'Padraig Darcy',
					occupation: 'Publican',
					mood: 'content',
					introduced: true,
					mood_emoji: '😌'
				}
			]);
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('textbox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();

			// Tab should select the mention, not trigger noun completion
			await fireEvent.keyDown(editor, { key: 'Tab' });
			const chip = editor.querySelector('.mention-chip');
			expect(chip).toBeTruthy();
			expect(chip?.textContent).toBe('@Padraig Darcy');
		});
	});
});
