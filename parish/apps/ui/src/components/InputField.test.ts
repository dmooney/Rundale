import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import { get } from 'svelte/store';
import {
	streamingActive,
	npcsHere,
	mapData,
	textLog,
	worldState,
} from '../stores/game';
import { findMatches, type KnownNoun } from '../stores/nouns';
import InputField from './InputField.svelte';

// Mock ipc submitInput
const mockSubmitInput = vi.fn(async (..._args: unknown[]) => {});
vi.mock('$lib/ipc', () => ({
	submitInput: (...args: unknown[]) => mockSubmitInput(...args),
}));

const localStore: Record<string, string> = {};
if (
	typeof localStorage === 'undefined' ||
	typeof localStorage.getItem !== 'function'
) {
	Object.defineProperty(globalThis, 'localStorage', {
		configurable: true,
		value: {
			getItem: (key: string) => localStore[key] ?? null,
			setItem: (key: string, value: string) => {
				localStore[key] = value;
			},
			clear: () => {
				for (const key of Object.keys(localStore)) delete localStore[key];
			},
		},
	});
}

const sessionStore: Record<string, string> = {};
if (
	typeof sessionStorage === 'undefined' ||
	typeof sessionStorage.getItem !== 'function'
) {
	Object.defineProperty(globalThis, 'sessionStorage', {
		configurable: true,
		value: {
			getItem: (key: string) => sessionStore[key] ?? null,
			setItem: (key: string, value: string) => {
				sessionStore[key] = value;
			},
			clear: () => {
				for (const key of Object.keys(sessionStore)) delete sessionStore[key];
			},
		},
	});
}

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

describe('InputField', () => {
	beforeEach(() => {
		streamingActive.set(false);
		npcsHere.set([]);
		mapData.set(null);
		textLog.set([]);
		mockSubmitInput.mockReset();
		mockSubmitInput.mockImplementation(async () => {});
		localStorage.clear?.();
		sessionStorage.clear?.();
	});

	it('renders an editable input area', () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('combobox');
		expect(editor).toBeTruthy();
		expect(editor.getAttribute('contenteditable')).toBe('true');
	});

	it('shows placeholder when empty', () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('combobox');
		expect(editor.dataset.placeholder).toBe(
			'What do you do? (@ to mention NPC)',
		);
	});

	it('is not editable when streaming', () => {
		streamingActive.set(true);
		const { getByRole } = render(InputField);
		const editor = getByRole('combobox');
		expect(editor.getAttribute('contenteditable')).toBe('false');
	});

	it('clears editor after submit', async () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('combobox');
		editor.textContent = 'hello';
		await fireEvent.input(editor);
		await fireEvent.keyDown(editor, { key: 'Enter' });
		expect(editor.textContent).toBe('');
	});

	it('enables send button when editor has text', async () => {
		const { getByRole } = render(InputField);
		const editor = getByRole('combobox');
		const sendBtn = getByRole('button', { name: /send/i }) as HTMLButtonElement;
		expect(sendBtn.disabled).toBe(true);

		editor.textContent = 'hello';
		await fireEvent.input(editor);
		expect(sendBtn.disabled).toBe(false);
	});

	// ── NPC mention autocomplete ────────────────────────────────────────

	describe('NPC mention autocomplete', () => {
		const testNpcs = [
			{
				name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				occupation: 'Publican',
				mood: 'content',
				introduced: true,
				mood_emoji: '😌',
			},
			{
				name: 'Siobhan Murphy',
				real_name: 'Siobhan Murphy',
				occupation: 'Farmer',
				mood: 'determined',
				introduced: true,
				mood_emoji: '😤',
			},
			{
				name: 'Father Callahan',
				real_name: 'Father Callahan',
				occupation: 'Priest',
				mood: 'serene',
				introduced: false,
				mood_emoji: '😌',
			},
		];

		beforeEach(() => {
			npcsHere.set(testNpcs);
		});

		it('shows mention dropdown when @ is typed with a letter', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			expect(queryByRole('listbox')).toBeNull();

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();
		});

		it('filters NPCs by typed text', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].textContent).toContain('Padraig Darcy');
		});

		it('shows matching NPCs for @S', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '@S');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].textContent).toContain('Siobhan Murphy');
		});

		it('does not show dropdown when no NPCs present', async () => {
			npcsHere.set([]);
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeNull();
		});

		it('dismisses dropdown on Escape', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();

			await fireEvent.keyDown(editor, { key: 'Escape' });
			expect(queryByRole('listbox')).toBeNull();
		});

		it('shows occupation for introduced NPCs', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options[0].textContent).toContain('Publican');
		});

		it('inserts a mention chip with full name on selection', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

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
		it('shows slash dropdown when / is typed', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/');
			await fireEvent.input(editor);
			const listbox = queryByRole('listbox');
			expect(listbox).toBeTruthy();
			expect(listbox?.getAttribute('aria-label')).toBe('Slash commands');
		});

		it('filters commands by typed text', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/pa');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].textContent).toContain('/pause');
		});

		it('navigates dropdown with ArrowDown/ArrowUp', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/he');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBe(1);
			expect(options[0].getAttribute('aria-selected')).toBe('true');
		});

		it('dismisses slash dropdown on Escape', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();

			await fireEvent.keyDown(editor, { key: 'Escape' });
			expect(queryByRole('listbox')).toBeNull();
		});

		it('selects no-arg command via Enter and submits', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/pa');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledWith('/pause');
		});

		it('selects arg command via Tab and inserts with trailing space', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/fo');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(queryByRole('listbox')).toBeNull();
			expect(editor.textContent).toContain('/fork ');
			expect(mockSubmitInput).not.toHaveBeenCalled();
		});

		it('does not show slash dropdown when / is mid-text', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, 'go to a/b');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeNull();
		});
	});

	// ── Model autocomplete (`/model …`) ─────────────────────────────────

	describe('model autocomplete', () => {
		it('shows model dropdown after `/model ` is typed', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model ');
			await fireEvent.input(editor);
			const listbox = queryByRole('listbox');
			expect(listbox).toBeTruthy();
			expect(listbox?.getAttribute('aria-label')).toBe('Model suggestions');
		});

		it('filters models by typed substring', async () => {
			const { getByRole, queryAllByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model claude');
			await fireEvent.input(editor);
			const options = queryAllByRole('option');
			expect(options.length).toBeGreaterThan(0);
			expect(
				options.every((o) => o.textContent?.toLowerCase().includes('claude')),
			).toBe(true);
		});

		it('Enter submits the typed text verbatim, not the highlighted suggestion', async () => {
			// User types a custom model ID that has substring overlap with catalog
			// entries (e.g. `claude-opus-99` would match `claude-opus-4-7`). Enter
			// must submit exactly what was typed so a partial / custom ID is never
			// silently swapped for a catalog match.
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model my-custom-fork');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledTimes(1);
			expect(mockSubmitInput.mock.calls[0][0]).toBe('/model my-custom-fork');
		});

		it('Enter on `/model ` (empty query) submits the show-current command', async () => {
			// Without this, an empty `/model ` + Enter would pick the first
			// catalog suggestion and silently change the active model.
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model ');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledTimes(1);
			expect(mockSubmitInput.mock.calls[0][0]).toBe('/model');
		});

		it('Tab picks the highlighted suggestion and submits it', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model claude');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(mockSubmitInput).toHaveBeenCalledTimes(1);
			const sent = mockSubmitInput.mock.calls[0][0];
			expect(sent).toMatch(/^\/model claude-/);
			expect(sent).not.toBe('/model claude');
		});

		it('Tab preserves the per-category prefix when picking a suggestion', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model.dialogue claude');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(mockSubmitInput).toHaveBeenCalledTimes(1);
			expect(mockSubmitInput.mock.calls[0][0]).toMatch(
				/^\/model\.dialogue claude-/,
			);
		});

		it('clears the editor after picking a model with Tab', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model claude');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(editor.textContent).toBe('');
		});

		it('Escape dismisses the model dropdown', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model ');
			await fireEvent.input(editor);
			expect(queryByRole('listbox')).toBeTruthy();

			await fireEvent.keyDown(editor, { key: 'Escape' });
			expect(queryByRole('listbox')).toBeNull();
		});

		it('does not show model dropdown for `/model` without a trailing space', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/model');
			await fireEvent.input(editor);
			// Without the trailing space we still get the slash dropdown matching `/model`,
			// but never the model-suggestions dropdown.
			const listbox = queryByRole('listbox');
			expect(listbox?.getAttribute('aria-label')).not.toBe('Model suggestions');
		});

		it('does not show model dropdown for unrelated commands', async () => {
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, '/provider llama3');
			await fireEvent.input(editor);
			const listbox = queryByRole('listbox');
			expect(listbox?.getAttribute('aria-label')).not.toBe('Model suggestions');
		});
	});

	// ── Input history ───────────────────────────────────────────────────

	describe('input history', () => {
		it('ArrowUp on empty editor with no history does nothing', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');
			await fireEvent.keyDown(editor, { key: 'ArrowUp' });
			expect(editor.textContent).toBe('');
		});

		it('recalls previous input with ArrowUp after submit', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

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
			const editor = getByRole('combobox');

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

		it('persists history to sessionStorage', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, 'persist me');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			const stored = JSON.parse(
				sessionStorage.getItem('parish-input-history') ?? '[]',
			);
			expect(stored).toContain('persist me');
		});

		it('does not store consecutive duplicate inputs', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, 'same');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			typeIntoEditor(editor, 'same');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			const stored = JSON.parse(
				sessionStorage.getItem('parish-input-history') ?? '[]',
			);
			expect(stored.filter((s: string) => s === 'same').length).toBe(1);
		});
	});

	// ── Multi-line input ────────────────────────────────────────────────

	describe('multi-line input', () => {
		it('Shift+Enter does not submit', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = 'line one';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter', shiftKey: true });

			// Editor should NOT be cleared (submit clears the editor)
			expect(editor.textContent).toBe('line one');
			expect(mockSubmitInput).not.toHaveBeenCalled();
		});

		it('Enter without Shift submits', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = 'submit me';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledWith('submit me', []);
		});
	});

	// ── Location quick-travel chips ─────────────────────────────────────

	describe('npc selection buttons', () => {
		const testNpcs = [
			{
				name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				occupation: 'Publican',
				mood: 'content',
				introduced: true,
				mood_emoji: '😌',
			},
			{
				name: 'an older man behind the bar',
				real_name: 'Tomas Brennan',
				occupation: 'Publican',
				mood: 'wary',
				introduced: false,
				mood_emoji: '😐',
			},
			{
				name: 'Siobhan Murphy',
				real_name: 'Siobhan Murphy',
				occupation: 'Farmer',
				mood: 'determined',
				introduced: true,
				mood_emoji: '😤',
			},
		];

		beforeEach(() => {
			npcsHere.set(testNpcs);
		});

		it('renders npc buttons and hides occupation for unintroduced npcs', () => {
			const { container, getByText } = render(InputField);
			expect(container.querySelectorAll('.npc-chip').length).toBe(3);
			expect(getByText('Padraig Darcy')).toBeTruthy();
			expect(getByText('Publican')).toBeTruthy();
			expect(
				(container.querySelectorAll('.npc-chip')[1] as HTMLElement).textContent,
			).not.toContain('Publican');
		});

		it('clicking an npc chip inserts an @name mention chip into the editor', async () => {
			const { container, getByRole } = render(InputField);
			const editor = getByRole('combobox');
			const chip = container.querySelector('.npc-chip') as HTMLButtonElement;
			await fireEvent.click(chip);

			const mention = editor.querySelector('.mention-chip');
			expect(mention).toBeTruthy();
			expect(mention?.textContent).toBe('@Padraig Darcy');
			expect((mention as HTMLElement)?.dataset.npc).toBe('Padraig Darcy');
		});

		it('syncs editorText after npc chip click so send button is enabled (#684)', async () => {
			const { container, getByRole } = render(InputField);
			const editor = getByRole('combobox');
			const sendBtn = getByRole('button', {
				name: /send/i,
			}) as HTMLButtonElement;
			expect(sendBtn.disabled).toBe(true);

			const chip = container.querySelector('.npc-chip') as HTMLButtonElement;
			await fireEvent.click(chip);

			// editorText must be synced synchronously — send button must be enabled.
			expect(sendBtn.disabled).toBe(false);
			// The DOM text representation must contain the NPC name.
			expect(editor.textContent).toContain('Padraig Darcy');
		});

		it('disables npc buttons during streaming but keeps them visible', () => {
			streamingActive.set(true);
			const { container } = render(InputField);
			expect(container.querySelector('.npc-chips')).toBeTruthy();
			const btn = container.querySelector('.npc-chip') as HTMLButtonElement;
			expect(btn.disabled).toBe(true);
		});
	});

	describe('quick-travel chips', () => {
		const testMapData = {
			locations: [
				{
					id: 'crossroads',
					name: 'The Crossroads',
					lat: 0,
					lon: 0,
					adjacent: false,
					hops: 0,
				},
				{
					id: 'pub',
					name: "Darcy's Pub",
					lat: 0.1,
					lon: 0.1,
					adjacent: true,
					hops: 1,
				},
				{
					id: 'church',
					name: 'The Church',
					lat: 0.2,
					lon: 0.2,
					adjacent: true,
					hops: 1,
				},
			],
			edges: [
				['crossroads', 'pub'],
				['crossroads', 'church'],
			] as [string, string][],
			player_location: 'crossroads',
			player_lat: 0,
			player_lon: 0,
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
			const chipTexts = Array.from(
				container.querySelectorAll('.travel-chip'),
			).map((el) => el.textContent);
			expect(chipTexts).not.toContain('The Crossroads');
		});

		it('clicking a chip submits movement command', async () => {
			mapData.set(testMapData);
			const { container } = render(InputField);
			const chip = container.querySelector('.travel-chip') as HTMLButtonElement;
			await fireEvent.click(chip);
			expect(mockSubmitInput).toHaveBeenCalledWith("go to Darcy's Pub");
		});

		it('disables chips during streaming but keeps them visible', () => {
			mapData.set(testMapData);
			streamingActive.set(true);
			const { container } = render(InputField);
			expect(container.querySelector('.travel-chips')).toBeTruthy();
			const btn = container.querySelector('.travel-chip') as HTMLButtonElement;
			expect(btn.disabled).toBe(true);
		});
	});

	// ── Paste handling ──────────────────────────────────────────────────

	describe('paste handling', () => {
		function placeCursorAtEnd(editor: HTMLElement) {
			const range = document.createRange();
			const sel = window.getSelection();
			range.selectNodeContents(editor);
			range.collapse(false);
			sel?.removeAllRanges();
			sel?.addRange(range);
		}

		function makePasteEvent(text: string): ClipboardEvent {
			// jsdom doesn't expose `DataTransfer`, so build a minimal stand-in
			// that only supports the `getData('text/plain')` call the paste
			// handler makes, and attach it via a non-enumerable property.
			const evt = new Event('paste', {
				bubbles: true,
				cancelable: true,
			}) as ClipboardEvent;
			Object.defineProperty(evt, 'clipboardData', {
				value: {
					getData: (type: string) => (type === 'text/plain' ? text : ''),
				},
			});
			return evt;
		}

		it('inserts pasted plain text into an empty editor', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox') as HTMLElement;
			editor.focus();
			placeCursorAtEnd(editor);

			const evt = makePasteEvent('hello world');
			editor.dispatchEvent(evt);

			expect(evt.defaultPrevented).toBe(true);
			expect(editor.textContent).toBe('hello world');
		});

		it('inserts pasted text at the cursor and keeps editorText state in sync (send enabled)', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox') as HTMLElement;
			const sendBtn = getByRole('button', {
				name: /send/i,
			}) as HTMLButtonElement;
			expect(sendBtn.disabled).toBe(true);

			editor.focus();
			placeCursorAtEnd(editor);
			editor.dispatchEvent(makePasteEvent('pasted'));

			// Wait a microtask for Svelte reactivity to settle.
			await Promise.resolve();
			expect(editor.textContent).toBe('pasted');
			expect(sendBtn.disabled).toBe(false);
		});

		it('pasting submits via Enter with the pasted content', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox') as HTMLElement;
			editor.focus();
			placeCursorAtEnd(editor);

			editor.dispatchEvent(makePasteEvent('typed via paste'));
			await Promise.resolve();
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledWith('typed via paste', []);
		});
	});

	// ── findMatches utility ─────────────────────────────────────────────

	describe('findMatches', () => {
		const testNouns: KnownNoun[] = [
			{ text: "Darcy's Pub", category: 'location', priority: 0 },
			{ text: 'The Crossroads', category: 'location', priority: 0 },
			{ text: 'The Church', category: 'location', priority: 2 },
			{ text: 'Padraig Darcy', category: 'npc', priority: 1 },
			{ text: 'Siobhan Murphy', category: 'npc', priority: 1 },
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
				{
					id: 'crossroads',
					name: 'The Crossroads',
					lat: 0,
					lon: 0,
					adjacent: true,
					hops: 1,
					visited: true,
				},
				{
					id: 'pub',
					name: "Darcy's Pub",
					lat: 0.1,
					lon: 0.1,
					adjacent: true,
					hops: 1,
					visited: true,
				},
				{
					id: 'church',
					name: 'The Church',
					lat: 0.2,
					lon: 0.2,
					adjacent: false,
					hops: 2,
					visited: true,
				},
				{
					id: 'mill',
					name: 'The Mill',
					lat: 0.3,
					lon: 0.3,
					adjacent: false,
					hops: 3,
					visited: false,
				},
			],
			edges: [
				['crossroads', 'pub'],
				['crossroads', 'church'],
			] as [string, string][],
			player_location: 'crossroads',
			player_lat: 0,
			player_lon: 0,
		};

		const testNpcs = [
			{
				name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				occupation: 'Publican',
				mood: 'content',
				introduced: true,
				mood_emoji: '😌',
			},
		];

		beforeEach(() => {
			mapData.set(testMapData);
			npcsHere.set(testNpcs);
		});

		it('Tab completes a matching prefix', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, 'go to pub');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(editor.textContent).toContain("Darcy's Pub");
		});

		it('Tab does nothing when no matches', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, 'go to xyz');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			expect(editor.textContent).toBe('go to xyz');
		});

		it('Tab cycles through all visited locations on empty input', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			await fireEvent.keyDown(editor, { key: 'Tab' });
			const first = editor.textContent ?? '';
			expect(first.length).toBeGreaterThan(0);

			await fireEvent.keyDown(editor, { key: 'Tab' });
			const second = editor.textContent ?? '';
			expect(second).not.toBe(first);
		});

		it('Tab completes unvisited (frontier) locations with lower priority', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			typeIntoEditor(editor, 'mill');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			// "The Mill" is unvisited but visible (frontier) — should complete
			expect(editor.textContent).toContain('The Mill');
		});

		it('Tab cycles through multiple matches', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			// "the" matches "The Crossroads" and "The Church"
			typeIntoEditor(editor, 'the');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Tab' });

			const firstMatch = editor.textContent;
			expect(
				firstMatch === 'The Crossroads' || firstMatch === 'The Church',
			).toBe(true);

			await fireEvent.keyDown(editor, { key: 'Tab' });
			const secondMatch = editor.textContent;
			expect(secondMatch).not.toBe(firstMatch);
			expect(
				secondMatch === 'The Crossroads' || secondMatch === 'The Church',
			).toBe(true);
		});

		it('typing resets completion state', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

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
					mood_emoji: '😌',
				},
			]);
			const { getByRole, queryByRole } = render(InputField);
			const editor = getByRole('combobox');

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

	// ── Submit error handling (#108) ────────────────────────────────────
	describe('submit error handling', () => {
		it('appends a system error entry when submitInput rejects', async () => {
			mockSubmitInput.mockImplementationOnce(async () => {
				throw new Error('network down');
			});
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = 'hello there';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			// Let the rejected promise settle so the catch handler runs.
			await Promise.resolve();
			await Promise.resolve();

			const log = get(textLog);
			const last = log[log.length - 1];
			expect(last.source).toBe('system');
			expect(last.subtype).toBe('error');
			expect(last.content).toContain('Could not send input');
			expect(last.content).toContain('network down');
		});

		it('appends an error entry when a quick-travel chip click fails', async () => {
			const testMap = {
				locations: [
					{
						id: 'crossroads',
						name: 'The Crossroads',
						lat: 0,
						lon: 0,
						adjacent: true,
						hops: 0,
						visited: true,
					},
					{
						id: 'pub',
						name: "Darcy's Pub",
						lat: 0.1,
						lon: 0.1,
						adjacent: true,
						hops: 1,
						visited: true,
					},
				],
				edges: [['crossroads', 'pub']] as [string, string][],
				player_location: 'crossroads',
				player_lat: 0,
				player_lon: 0,
			};
			mapData.set(testMap);
			mockSubmitInput.mockImplementationOnce(async () => {
				throw new Error('server busy');
			});

			const { container } = render(InputField);
			const chip = container.querySelector('.travel-chip') as HTMLButtonElement;
			await fireEvent.click(chip);
			await Promise.resolve();
			await Promise.resolve();

			const log = get(textLog);
			const last = log[log.length - 1];
			expect(last.subtype).toBe('error');
			expect(last.content).toContain("Could not travel to Darcy's Pub");
			expect(last.content).toContain('server busy');
		});
	});

	// ── Auto-resume on send ──────────────────────────────────────────

	describe('Auto-resume on send', () => {
		beforeEach(() => {
			worldState.set({
				paused: true,
				inference_paused: false,
				paused_game_time: '12:00',
				location_id: 'crossroads',
				location_name: 'The Crossroads',
			} as any);
		});

		it('does not resume game just by typing non-slash command while paused', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = 'h';
			await fireEvent.input(editor);

			expect(mockSubmitInput).not.toHaveBeenCalledWith('/resume');
		});

		it('resumes game when sending non-slash command while paused', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = 'hello';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).toHaveBeenCalledWith('/resume');
			expect(mockSubmitInput).toHaveBeenCalledWith('hello', []);
		});

		it('does not resume game when sending a slash command while paused', async () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = '/help';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).not.toHaveBeenCalledWith('/resume');
			expect(mockSubmitInput).toHaveBeenCalledWith('/help');
		});

		it('does not resume game if not paused', async () => {
			worldState.set({ paused: false } as any);
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');

			editor.textContent = 'hello';
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });

			expect(mockSubmitInput).not.toHaveBeenCalledWith('/resume');
			expect(mockSubmitInput).toHaveBeenCalledWith('hello', []);
		});
	});

	// ── ARIA: combobox + listbox attributes (#683) ──────────────────────────
	describe('ARIA combobox attributes (#683)', () => {
		const testNpcs = [
			{
				name: 'Padraig Darcy',
				real_name: 'Padraig Darcy',
				occupation: 'Publican',
				mood: 'content',
				introduced: true,
				mood_emoji: '😌',
			},
		];

		it('editor has aria-haspopup="listbox" and aria-expanded=false when closed', () => {
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');
			expect(editor.getAttribute('aria-haspopup')).toBe('listbox');
			expect(editor.getAttribute('aria-expanded')).toBe('false');
		});

		it('aria-expanded becomes true when mention dropdown opens', async () => {
			npcsHere.set(testNpcs);
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');
			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			expect(editor.getAttribute('aria-expanded')).toBe('true');
		});

		it('mention chip inserted via selectNpc has role="img" and aria-label', async () => {
			npcsHere.set(testNpcs);
			const { getByRole } = render(InputField);
			const editor = getByRole('combobox');
			typeIntoEditor(editor, '@P');
			await fireEvent.input(editor);
			await fireEvent.keyDown(editor, { key: 'Enter' });
			const chip = editor.querySelector('.mention-chip') as HTMLElement;
			expect(chip.getAttribute('role')).toBe('img');
			expect(chip.getAttribute('aria-label')).toBe('Mention: Padraig Darcy');
		});

		it('mention chip inserted via npc-chip button has role="img" and aria-label', async () => {
			npcsHere.set(testNpcs);
			const { container, getByRole } = render(InputField);
			const editor = getByRole('combobox');
			const npcBtn = container.querySelector('.npc-chip') as HTMLButtonElement;
			await fireEvent.click(npcBtn);
			const chip = editor.querySelector('.mention-chip') as HTMLElement;
			expect(chip.getAttribute('role')).toBe('img');
			expect(chip.getAttribute('aria-label')).toBe('Mention: Padraig Darcy');
		});
	});
});
