import { describe, expect, it, vi } from 'vitest';
import {
	appendNotebookCommandHistory,
	draftForNotebookAction,
	loadNotebookCommandHistory,
	NOTEBOOK_COMMAND_PLACEHOLDER,
	NOTEBOOK_COMMAND_HISTORY_MAX,
	resolveNotebookCommandPresentation,
	saveNotebookCommandHistory,
	submitNotebookCommand,
	windowNotebookCommandText,
} from './command';
import type { NotebookCommandState } from '$lib/illustrated-parish/types';

const roisin = {
	npc_id: 6,
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};

describe('illustrated notebook command input', () => {
	function visualState(
		overrides: Partial<NotebookCommandState> = {},
	): NotebookCommandState {
		return {
			text: '',
			focused: false,
			busy: false,
			disabled: false,
			error: null,
			...overrides,
		};
	}

	it('resolves distinct idle, focus, text, busy, disabled, and error presentations', () => {
		expect(resolveNotebookCommandPresentation(visualState())).toMatchObject({
			phase: 'idle',
			displayText: NOTEBOOK_COMMAND_PLACEHOLDER,
			statusText: null,
			showCaret: false,
			sendDisabled: true,
		});
		expect(
			resolveNotebookCommandPresentation(visualState({ focused: true })),
		).toMatchObject({
			phase: 'focused',
			statusText: 'Writing',
			showCaret: true,
		});
		expect(
			resolveNotebookCommandPresentation(
				visualState({ text: 'ask Roisin', focused: true }),
			),
		).toMatchObject({
			phase: 'typing',
			displayText: 'ask Roisin',
			showCaret: true,
			sendDisabled: false,
		});
		expect(
			resolveNotebookCommandPresentation(visualState({ busy: true })),
		).toMatchObject({
			phase: 'busy',
			displayText: 'waiting on the parish...',
			statusText: 'Parish reply in progress',
			showCaret: false,
			sendDisabled: true,
		});
		expect(
			resolveNotebookCommandPresentation(visualState({ disabled: true })),
		).toMatchObject({
			phase: 'disabled',
			displayText: 'setting ink to paper...',
			statusText: 'Sending your line',
			showCaret: false,
			sendDisabled: true,
		});
		expect(
			resolveNotebookCommandPresentation(
				visualState({
					text: 'look',
					focused: true,
					error: '  bridge   unavailable ',
				}),
			),
		).toMatchObject({
			phase: 'error',
			displayText: 'look',
			statusText: 'Ink blotted — bridge unavailable',
			showCaret: true,
			sendDisabled: false,
		});
	});

	it('gives busy and error precedence without discarding the written line', () => {
		expect(
			resolveNotebookCommandPresentation(
				visualState({
					text: 'ask Roisin',
					busy: true,
					disabled: true,
				}),
			),
		).toMatchObject({
			phase: 'busy',
			displayText: 'ask Roisin',
		});
		expect(
			resolveNotebookCommandPresentation(
				visualState({ busy: true, error: 'connection lost' }),
			),
		).toMatchObject({
			phase: 'error',
			statusText: 'Ink blotted — connection lost',
		});
	});

	it('keeps the newest command characters visible at desktop and mobile widths', () => {
		const command =
			'ask Roisin to recount every detail because the latest typing stays visible';
		const desktop = windowNotebookCommandText(command, 58);
		const mobile = windowNotebookCommandText(command, 30);

		expect(desktop).toHaveLength(58);
		expect(desktop).toMatch(/^\.\.\./);
		expect(desktop).toMatch(/the latest typing stays visible$/);
		expect(mobile).toHaveLength(30);
		expect(mobile).toMatch(/^\.\.\./);
		expect(mobile).toBe('...latest typing stays visible');
		expect(windowNotebookCommandText('look around', 30)).toBe('look around');
	});

	it('seeds action stamps from the selected person', () => {
		expect(draftForNotebookAction('ask', roisin)).toBe('ask Roisin Connolly ');
		expect(draftForNotebookAction('observe', roisin)).toBe(
			'observe Roisin Connolly',
		);
		expect(draftForNotebookAction('talk', null)).toBe('talk to ');
	});

	it('keeps a bounded, consecutive-deduplicated notebook-only command history', () => {
		const history = appendNotebookCommandHistory([], ' look around ');
		expect(history).toEqual(['look around']);
		expect(appendNotebookCommandHistory(history, 'look around')).toEqual([
			'look around',
		]);
		expect(appendNotebookCommandHistory(history, 'talk to Roisin')).toEqual([
			'look around',
			'talk to Roisin',
		]);

		const bounded = appendNotebookCommandHistory(
			Array.from(
				{ length: NOTEBOOK_COMMAND_HISTORY_MAX },
				(_value, index) => `command ${index}`,
			),
			'latest command',
		);
		expect(bounded).toHaveLength(NOTEBOOK_COMMAND_HISTORY_MAX);
		expect(bounded[0]).toBe('command 1');
		expect(bounded.at(-1)).toBe('latest command');
	});

	it('loads valid session history and ignores corrupted storage', () => {
		const values = new Map<string, string>();
		const storage = {
			getItem: (key: string) => values.get(key) ?? null,
			setItem: (key: string, value: string) => values.set(key, value),
		};

		saveNotebookCommandHistory(['look around', 'talk to Roisin'], storage);
		expect(loadNotebookCommandHistory(storage)).toEqual([
			'look around',
			'talk to Roisin',
		]);
		values.set('parish-notebook-command-history', '{broken json');
		expect(loadNotebookCommandHistory(storage)).toEqual([]);
	});

	it('submits natural-language text through the existing submit function', async () => {
		const submitInput = vi.fn(async () => {});
		const onLocalSubmit = vi.fn();

		await expect(
			submitNotebookCommand({
				text: ' ask Roisin what she saw ',
				busy: false,
				paused: false,
				submitInput,
				onLocalSubmit,
			}),
		).resolves.toBe(true);

		expect(onLocalSubmit).toHaveBeenCalledOnce();
		expect(submitInput).toHaveBeenCalledWith('ask Roisin what she saw');
	});

	it('resumes paused worlds before non-system commands', async () => {
		const submitInput = vi.fn(async () => {});

		await submitNotebookCommand({
			text: 'look around',
			busy: false,
			paused: true,
			submitInput,
			onLocalSubmit: vi.fn(),
		});

		expect(submitInput).toHaveBeenNthCalledWith(1, '/resume');
		expect(submitInput).toHaveBeenNthCalledWith(2, 'look around');
	});

	it('does not submit while busy or empty', async () => {
		const submitInput = vi.fn(async () => {});

		await expect(
			submitNotebookCommand({
				text: '   ',
				busy: false,
				paused: false,
				submitInput,
				onLocalSubmit: vi.fn(),
			}),
		).resolves.toBe(false);
		await expect(
			submitNotebookCommand({
				text: 'look',
				busy: true,
				paused: false,
				submitInput,
				onLocalSubmit: vi.fn(),
			}),
		).resolves.toBe(false);

		expect(submitInput).not.toHaveBeenCalled();
	});
});
