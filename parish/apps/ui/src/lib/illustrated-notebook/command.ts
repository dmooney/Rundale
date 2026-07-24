import type { NpcInfo } from '$lib/types';
import type {
	NotebookCommandPresentation,
	NotebookCommandState,
} from '$lib/illustrated-parish/types';
import {
	notebookActionDraft,
	type NotebookAction,
} from '$lib/notebook/actions';

export type SubmitInput = (text: string) => Promise<void>;

export const NOTEBOOK_COMMAND_PLACEHOLDER = 'ask Roisin what she saw';
export const NOTEBOOK_COMMAND_HISTORY_STORAGE_KEY =
	'parish-notebook-command-history';
export const NOTEBOOK_COMMAND_HISTORY_MAX = 50;

type NotebookCommandHistoryStorage = Pick<Storage, 'getItem' | 'setItem'>;

export interface SubmitNotebookCommandOptions {
	text: string;
	busy: boolean;
	paused: boolean;
	submitInput: SubmitInput;
	onLocalSubmit: () => void;
}

function sessionHistoryStorage(): NotebookCommandHistoryStorage | null {
	if (typeof window === 'undefined') return null;
	try {
		return window.sessionStorage;
	} catch {
		return null;
	}
}

export function loadNotebookCommandHistory(
	storage: Pick<Storage, 'getItem'> | null = sessionHistoryStorage(),
): string[] {
	if (!storage) return [];
	try {
		const encoded = storage.getItem(NOTEBOOK_COMMAND_HISTORY_STORAGE_KEY);
		if (!encoded) return [];
		const parsed: unknown = JSON.parse(encoded);
		if (!Array.isArray(parsed)) return [];

		const history: string[] = [];
		for (const value of parsed) {
			if (typeof value !== 'string') continue;
			const command = value.trim();
			if (!command || history[history.length - 1] === command) continue;
			history.push(command);
		}
		return history.slice(-NOTEBOOK_COMMAND_HISTORY_MAX);
	} catch {
		return [];
	}
}

export function appendNotebookCommandHistory(
	history: readonly string[],
	command: string,
): string[] {
	const trimmed = command.trim();
	if (!trimmed || history[history.length - 1] === trimmed) return [...history];
	return [...history, trimmed].slice(-NOTEBOOK_COMMAND_HISTORY_MAX);
}

export function saveNotebookCommandHistory(
	history: readonly string[],
	storage: Pick<Storage, 'setItem'> | null = sessionHistoryStorage(),
): void {
	if (!storage) return;
	try {
		storage.setItem(
			NOTEBOOK_COMMAND_HISTORY_STORAGE_KEY,
			JSON.stringify(history),
		);
	} catch {
		// A private or quota-limited browser session must not make command entry fail.
	}
}

export function draftForNotebookAction(
	action: NotebookAction,
	selectedNpc: NpcInfo | null,
): string {
	return notebookActionDraft(action, selectedNpc);
}

export function windowNotebookCommandText(
	text: string,
	maxChars: number,
): string {
	const characters = Array.from(text);
	if (characters.length <= maxChars) return text;
	if (maxChars <= 3) return '.'.repeat(Math.max(0, maxChars));
	return `...${characters.slice(-(maxChars - 3)).join('')}`;
}

export function resolveNotebookCommandPresentation(
	state: NotebookCommandState,
): NotebookCommandPresentation {
	const text = state.text.trim();
	const error = state.error?.replace(/\s+/g, ' ').trim() || null;
	const phase = error
		? 'error'
		: state.busy
			? 'busy'
			: state.disabled
				? 'disabled'
				: text
					? 'typing'
					: state.focused
						? 'focused'
						: 'idle';

	const displayText =
		state.text ||
		(phase === 'busy'
			? 'waiting on the parish...'
			: phase === 'disabled'
				? 'setting ink to paper...'
				: NOTEBOOK_COMMAND_PLACEHOLDER);
	const statusText = error
		? `Ink blotted — ${error}`
		: state.busy
			? 'Parish reply in progress'
			: state.disabled
				? 'Sending your line'
				: state.focused
					? 'Writing'
					: null;

	return {
		phase,
		displayText,
		statusText,
		showCaret: state.focused && !state.busy && !state.disabled,
		sendDisabled: state.busy || state.disabled || !text,
	};
}

export async function submitNotebookCommand({
	text,
	busy,
	paused,
	submitInput,
	onLocalSubmit,
}: SubmitNotebookCommandOptions): Promise<boolean> {
	const trimmed = text.trim();
	if (!trimmed || busy) return false;

	if (paused && !trimmed.startsWith('/')) {
		await submitInput('/resume');
	}
	onLocalSubmit();
	await submitInput(trimmed);
	return true;
}
