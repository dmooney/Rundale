<script lang="ts">
	import { streamingActive, npcsHere, mapData, pushErrorLog, formatIpcError, worldState } from '../stores/game';
	import { submitInput } from '$lib/ipc';
	import { filterCommands, type SlashCommand } from '$lib/slash-commands';
	import {
		detectModelTrigger,
		filterModels,
		type ModelSuggestion
	} from '$lib/model-catalog';
	import { knownNouns, findMatches, type KnownNoun } from '../stores/nouns';
	import { get } from 'svelte/store';
	import MoodIcon from './MoodIcon.svelte';

	let editorEl: HTMLDivElement;
	let editorText = $state('');

	// ── Unified dropdown state ──────────────────────────────────────────────
	type DropdownMode = 'mention' | 'slash' | 'model' | null;
	let dropdownMode: DropdownMode = $state(null);
	let selectedIndex = $state(0);
	let mentionQuery = $state('');
	let slashQuery = $state('');
	// `/model` autocomplete: the leading command prefix (`/model`,
	// `/model.dialogue`, …) and the partial model name typed after it.
	let modelPrefix = $state('/model');
	let modelQuery = $state('');

	const filteredNpcs = $derived(
		mentionQuery === ''
			? $npcsHere
			: $npcsHere.filter((npc) =>
					npc.name.toLowerCase().startsWith(mentionQuery.toLowerCase())
				)
	);

	const filteredCommands = $derived(filterCommands(slashQuery));

	const filteredModels = $derived(filterModels(modelQuery));

	// ── Input history ───────────────────────────────────────────────────────
	const HISTORY_KEY = 'parish-input-history';
	const HISTORY_MAX = 50;

	function loadHistory(): string[] {
		// sessionStorage (not localStorage) — input history may contain sensitive user typing; limit to tab lifetime
		try {
			const raw = sessionStorage.getItem(HISTORY_KEY);
			if (raw) return JSON.parse(raw);
		} catch { /* ignore corrupt data */ }
		return [];
	}

	function saveHistory(h: string[]) {
		// sessionStorage (not localStorage) — input history may contain sensitive user typing; limit to tab lifetime
		try { sessionStorage.setItem(HISTORY_KEY, JSON.stringify(h)); } catch { /* quota */ }
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

	// Chip selection is keyed by `real_name` (the canonical id) so unintroduced
	// NPCs whose `name` is a placeholder ("a stern priest") still resolve
	// correctly on the backend.
	let selectedNpcRealNames = $state<string[]>([]);

	$effect(() => {
		const visible = new Set($npcsHere.map((npc) => npc.real_name));
		const pruned = selectedNpcRealNames.filter((name) => visible.has(name));
		if (
			pruned.length !== selectedNpcRealNames.length ||
			pruned.some((name, i) => name !== selectedNpcRealNames[i])
		) {
			selectedNpcRealNames = pruned;
		}
	});

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
			editorEl.textContent = '';
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
		if (dropdownMode === 'model' && selectedIndex >= filteredModels.length) {
			selectedIndex = Math.max(0, filteredModels.length - 1);
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
		return editorText.trim() === '';
	}

	function syncEditorText() {
		editorText = getPlainText();
	}

	/** Clears the editor content. */
	function clearEditor() {
		if (editorEl) {
			editorEl.textContent = '';
		}
		editorText = '';
	}

	/** Sets the editor's plain-text content and places cursor at end. */
	function setEditorText(text: string) {
		if (!editorEl) return;
		editorEl.textContent = text;
		editorText = text;
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

	// ── Model autocomplete (`/model …`, `/model.<category> …`) ──────────────

	function detectModel() {
		const trigger = detectModelTrigger(getPlainText());
		if (trigger === null) {
			if (dropdownMode === 'model') dropdownMode = null;
			return;
		}
		modelPrefix = trigger.prefix;
		modelQuery = trigger.query;
		dropdownMode = 'model';
		selectedIndex = 0;
	}

	function selectModelSuggestion(suggestion: ModelSuggestion) {
		const command = `${modelPrefix} ${suggestion.name}`;
		clearEditor();
		dropdownMode = null;
		submitInput(command).catch((err) => {
			pushErrorLog(`Could not send "${command}": ${formatIpcError(err)}`);
		});
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
			editorEl.textContent = '';
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
			syncEditorText();
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
		syncEditorText();
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
			submitInput(cmd.command).catch((err) => {
				pushErrorLog(`Could not send "${cmd.command}": ${formatIpcError(err)}`);
			});
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
		syncEditorText();
	}

	// ── Transport icons ──────────────────────────────────────────────────────

	const TRANSPORT_ICON_PATHS: Record<string, string> = {
		walking:
			'M152,80a32,32,0,1,0-32-32A32,32,0,0,0,152,80Zm0-48a16,16,0,1,1-16,16A16,16,0,0,1,152,32Zm64,112a8,8,0,0,1-8,8c-35.31,0-52.95-17.81-67.12-32.12-2.74-2.77-5.36-5.4-8-7.84l-13.43,30.88,37.2,26.57A8,8,0,0,1,160,176v56a8,8,0,0,1-16,0V180.12l-31.07-22.2L79.34,235.19A8,8,0,0,1,72,240a7.84,7.84,0,0,1-3.19-.67,8,8,0,0,1-4.15-10.52l54.08-124.37c-9.31-1.65-20.92,1.2-34.7,8.58a163.88,163.88,0,0,0-30.57,21.77,8,8,0,0,1-10.95-11.66c2.5-2.35,61.69-57.23,98.72-25.08,3.83,3.32,7.48,7,11,10.57C166.19,122.7,179.36,136,208,136A8,8,0,0,1,216,144Z',
		jaunting_car:
			'M232,136a8,8,0,0,0-8-8H195.7L133.86,59.06A16,16,0,0,0,121,52H40A16,16,0,0,0,24,68V172a8,8,0,0,0,8,8H49a36,36,0,0,0,70,0h58a36,36,0,0,0,70,0h7a8,8,0,0,0,8-8V152A16.09,16.09,0,0,0,232,136ZM121,68l55.93,72H121ZM84,196a20,20,0,1,1-20-20A20,20,0,0,1,84,196Zm128,0a20,20,0,1,1-20-20A20,20,0,0,1,212,196Z'
	};

	function transportIconPath(id: string | undefined): string | undefined {
		return id ? TRANSPORT_ICON_PATHS[id] : undefined;
	}

	// ── Quick-travel ────────────────────────────────────────────────────────

	async function quickTravel(locationName: string) {
		if ($streamingActive) return;
		// #354: if the player is mid-composition, don't clobber their
		// draft. The quick-travel chip is an explicit nav action, but
		// losing work silently (and without saving to history so ArrowUp
		// can't recover it) is worse than forcing the user to either
		// send or clear their draft first. Surface a clear reminder and
		// bail out.
		//
		// codex P2 on #573: isEditorEmpty() reads the cached editorText,
		// which can be stale when the DOM was modified programmatically
		// (e.g. insertNpcMention drops in chips without firing input
		// events that re-sync). Pull a fresh plain-text view first so a
		// non-empty draft can't sneak past this guard.
		syncEditorText();
		if (!isEditorEmpty()) {
			pushErrorLog(
				`Send or clear the draft in the input before travelling to ${locationName}.`
			);
			return;
		}
		selectedNpcRealNames = [];
		try {
			await submitInput(`go to ${locationName}`);
		} catch (err) {
			pushErrorLog(
				`Could not travel to ${locationName}: ${formatIpcError(err)}`
			);
		}
	}

	function toggleNpcSelection(realName: string) {
		if ($streamingActive) return;
		if (selectedNpcRealNames.includes(realName)) {
			selectedNpcRealNames = selectedNpcRealNames.filter((name) => name !== realName);
		} else {
			selectedNpcRealNames = [...selectedNpcRealNames, realName];
		}
		editorEl?.focus();
	}

	function insertNpcMention(npcName: string) {
		if ($streamingActive || !editorEl) return;

		const chip = document.createElement('span');
		chip.className = 'mention-chip';
		chip.contentEditable = 'false';
		chip.dataset.npc = npcName;
		chip.textContent = `@${npcName}`;

		const trailing = document.createTextNode('\u00A0');
		const sel = window.getSelection();

		if (sel && sel.rangeCount > 0 && editorEl.contains(sel.getRangeAt(0).startContainer)) {
			const range = sel.getRangeAt(0);
			range.deleteContents();
			range.insertNode(trailing);
			range.insertNode(chip);
		} else {
			editorEl.appendChild(chip);
			editorEl.appendChild(trailing);
		}

		const range = document.createRange();
		range.setStart(trailing, 1);
		range.collapse(true);
		sel?.removeAllRanges();
		sel?.addRange(range);
		editorEl.focus();
	}

	// ── Submit ──────────────────────────────────────────────────────────────

	let isSubmitting = $state(false);

	async function handleSubmit(e: Event) {
		e.preventDefault();
		// If a mention or slash dropdown is open, Enter selects the highlighted
		// item — there's no scenario where the player wants to submit half-typed
		// `@P` or `/pa` literally. The model dropdown is intentionally excluded:
		// `/model …` is itself a valid command (e.g. `/model ` shows the current
		// model and `/model my-custom` sets a non-catalog ID), so Enter must
		// always submit exactly what the player typed. Tab and click remain the
		// explicit ways to pick a catalog suggestion for `/model`.
		if (dropdownMode === 'mention' && filteredNpcs.length > 0) {
			selectNpc(filteredNpcs[selectedIndex].name);
			return;
		}
		if (dropdownMode === 'slash' && filteredCommands.length > 0) {
			selectSlashCommand(filteredCommands[selectedIndex]);
			return;
		}
		syncEditorText();
		const trimmed = editorText.trim();
		if (!trimmed || $streamingActive || isSubmitting) return;

		isSubmitting = true;

		// If the game is paused and this is not a system command, resume
		// before sending the input so the world ticks as expected (#831).
		if ($worldState?.paused && !trimmed.startsWith('/')) {
			try {
				await submitInput('/resume');
			} catch {
				// Fall through — if resume fails, still try to send input
			}
		}

		clearEditor();
		dropdownMode = null;

		// Push to history (skip consecutive dupes)
		if (history.length === 0 || history[history.length - 1] !== trimmed) {
			history = [...history.slice(-(HISTORY_MAX - 1)), trimmed];
			saveHistory(history);
		}
		historyIndex = -1;

		const addressedTo = [...selectedNpcRealNames];
		selectedNpcRealNames = [];
		try {
			await submitInput(trimmed, addressedTo);
		} catch (err) {
			pushErrorLog(`Could not send input: ${formatIpcError(err)}`);
		} finally {
			isSubmitting = false;
		}
	}

	// ── Keyboard handling ───────────────────────────────────────────────────

	function handleKeydown(e: KeyboardEvent) {
		// Dropdown navigation (mention, slash, or model)
		if (dropdownMode !== null) {
			const items =
				dropdownMode === 'mention'
					? filteredNpcs
					: dropdownMode === 'slash'
						? filteredCommands
						: filteredModels;
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
					} else if (dropdownMode === 'slash') {
						selectSlashCommand(filteredCommands[selectedIndex]);
					} else {
						selectModelSuggestion(filteredModels[selectedIndex]);
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
			detectModel();
			if (dropdownMode !== 'model') {
				detectSlash();
			}
		}
		syncEditorText();
	}

	// Prevent pasting rich content — only plain text.
	// `document.execCommand('insertText')` is deprecated and doesn't
	// always fire the `input` event, which would leave `editorText` stale.
	// We insert a text node at the current selection manually and then
	// sync the reactive state ourselves.
	function handlePaste(e: ClipboardEvent) {
		e.preventDefault();
		const text = e.clipboardData?.getData('text/plain') ?? '';
		if (!text || !editorEl) return;

		const sel = window.getSelection();
		if (!sel || sel.rangeCount === 0) {
			// No selection (e.g. editor never focused) — append to end.
			editorEl.appendChild(document.createTextNode(text));
		} else {
			const range = sel.getRangeAt(0);
			// Only insert if the cursor is inside the editor.
			if (!editorEl.contains(range.startContainer)) return;
			range.deleteContents();
			const node = document.createTextNode(text);
			range.insertNode(node);
			// Move cursor to the end of the inserted text.
			range.setStartAfter(node);
			range.collapse(true);
			sel.removeAllRanges();
			sel.addRange(range);
		}

		// execCommand used to fire 'input'; insertNode does not, so keep
		// editorText in sync and re-run input-driven logic (mention/slash
		// detection, history/completion resets) explicitly.
		if (historyIndex >= 0) historyIndex = -1;
		if (completion.active) resetCompletion();
		detectMention();
		if (dropdownMode !== 'mention') {
			detectModel();
			if (dropdownMode !== 'model') detectSlash();
		}
		syncEditorText();
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
	{#if dropdownMode === 'model' && filteredModels.length > 0}
		<ul class="mention-dropdown" role="listbox" aria-label="Model suggestions">
			{#each filteredModels as model, i}
				<li
					role="option"
					aria-selected={i === selectedIndex}
					class="mention-item"
					class:selected={i === selectedIndex}
					onmousedown={(e) => { e.preventDefault(); selectModelSuggestion(model); }}
					onmouseenter={() => (selectedIndex = i)}
				>
					<span class="mention-name">{model.name}</span>
					<span class="mention-detail">{model.provider}</span>
				</li>
			{/each}
		</ul>
	{/if}
	{#if $npcsHere.length > 0}
		<div class="npc-chips" data-testid="npc-chips">
			<span class="npc-label">Speak To</span>
			{#each $npcsHere as npc}
				<button
					class="npc-chip"
					aria-label="Speak to {npc.name}"
					disabled={$streamingActive}
					onclick={() => insertNpcMention(npc.name)}
				>
					<span class="npc-chip-mood"><MoodIcon mood={npc.mood} /></span>
					<span class="npc-chip-copy">
						<span class="npc-chip-name">{npc.name}</span>
						{#if npc.introduced}
							<span class="npc-chip-detail">{npc.occupation}</span>
						{/if}
					</span>
				</button>
			{/each}
		</div>
	{/if}
	{#if adjacentLocations.length > 0}
		<div class="travel-chips">
			<span class="travel-label">Go To</span>
			{#each adjacentLocations as loc}
				<button
					class="travel-chip"
					aria-label="Travel to {loc.name}{loc.travel_minutes !== undefined ? `, ${loc.travel_minutes} minute walk` : ''}"
					onclick={() => quickTravel(loc.name)}
					disabled={$streamingActive}
				>
					{loc.name}
					{#if loc.travel_minutes !== undefined}
						<span class="chip-meta">
							{loc.travel_minutes}m
							{#if transportIconPath($mapData?.transport_id)}
								<svg viewBox="0 0 256 256" class="transport-icon" aria-hidden="true">
									<path d={transportIconPath($mapData?.transport_id)} />
								</svg>
							{/if}
						</span>
					{/if}
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
				aria-disabled={$streamingActive}
				data-testid="input-field"
				onkeydown={handleKeydown}
				oninput={handleInput}
				onpaste={handlePaste}
				data-placeholder={$streamingActive ? 'Waiting…' : 'What do you do? (@ to mention NPC)'}
			></div>
		</div>
		<button type="button" onclick={handleSubmit} disabled={$streamingActive || isEditorEmpty()} class="send-btn"
			aria-label={$streamingActive ? 'Waiting for response…' : isEditorEmpty() ? 'Type a message to send' : 'Send message (Enter)'}
		>
			Send
		</button>
	</div>
</div>

<style>
	.input-wrapper {
		position: sticky;
		bottom: 0;
		flex: 0 0 auto;
		z-index: 25;
		padding-bottom: env(safe-area-inset-bottom);
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
		border-radius: 2px;
		color: var(--color-fg);
		padding: 0.5rem 0.75rem;
		font-size: 0.95rem;
		font-family: var(--font-body);
		font-style: italic;
		outline: none;
		max-height: 6em;
		overflow-y: auto;
		white-space: pre-wrap;
		word-wrap: break-word;
		overflow-wrap: break-word;
		transition: border-color 0.2s;
		-webkit-user-select: text;
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
		padding: 0.5rem 1.1rem;
		font-size: 0.65rem;
		font-family: var(--font-display);
		font-weight: 600;
		letter-spacing: 0.12em;
		text-transform: uppercase;
		border-radius: 2px;
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

	.npc-chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.45rem;
		padding: 0.45rem 0.75rem;
		background:
			linear-gradient(180deg, color-mix(in srgb, var(--color-panel-bg) 88%, var(--color-accent) 12%), var(--color-panel-bg));
		border-top: 1px solid var(--color-border);
	}

	.npc-label {
		color: var(--color-muted);
		font-size: 0.6rem;
		font-family: var(--font-display);
		letter-spacing: 0.08em;
		text-transform: uppercase;
		opacity: 0.8;
		flex-shrink: 0;
	}

	.npc-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.45rem;
		min-width: 0;
		padding: 0.35rem 0.55rem;
		border: 1px solid color-mix(in srgb, var(--color-accent) 30%, var(--color-border));
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-panel-bg) 80%, var(--color-bg));
		color: var(--color-fg);
		cursor: pointer;
		text-align: left;
		transition: background 0.15s, border-color 0.15s, transform 0.15s, color 0.15s;
	}

	.npc-chip:hover:not(:disabled),
	.npc-chip:focus-visible:not(:disabled) {
		border-color: color-mix(in srgb, var(--color-accent) 60%, var(--color-border));
		transform: translateY(-1px);
	}

	.npc-chip:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.npc-chip-mood {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: 1rem;
		flex-shrink: 0;
	}

	.npc-chip-copy {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.npc-chip-name {
		font-size: 0.78rem;
		line-height: 1.1;
	}

	.npc-chip-detail {
		font-size: 0.62rem;
		color: var(--color-muted);
		line-height: 1.1;
	}

	.travel-chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.4rem;
		padding: 0.4rem 0.75rem;
		background: var(--color-panel-bg);
		border-top: 1px solid var(--color-border);
	}

	.travel-label {
		color: var(--color-muted);
		font-size: 0.6rem;
		font-family: var(--font-display);
		letter-spacing: 0.08em;
		text-transform: uppercase;
		opacity: 0.7;
		flex-shrink: 0;
	}

	.travel-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		background: transparent;
		color: var(--color-muted);
		border: 1px solid var(--color-border);
		border-radius: 2px;
		padding: 0.2rem 0.55rem;
		font-size: 0.64rem;
		font-family: var(--font-display);
		letter-spacing: 0.06em;
		text-transform: uppercase;
		cursor: pointer;
		transition: background 0.15s, color 0.15s, border-color 0.15s;
	}

	.travel-chip:hover:not(:disabled),
	.travel-chip:focus-visible:not(:disabled) {
		background: var(--color-accent);
		color: var(--color-bg);
		border-color: var(--color-accent);
	}

	.travel-chip:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.chip-meta {
		display: inline-flex;
		align-items: center;
		gap: 0.2rem;
		opacity: 0.7;
		font-size: 0.58rem;
	}

	.transport-icon {
		width: 0.75rem;
		height: 0.75rem;
		fill: currentColor;
		vertical-align: middle;
	}

	@media (max-width: 768px) {
		.input-field {
			font-size: 16px;
			line-height: 1.4;
		}
	}
</style>
