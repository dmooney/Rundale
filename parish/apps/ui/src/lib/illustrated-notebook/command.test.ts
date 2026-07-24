import { describe, expect, it, vi } from 'vitest';
import { draftForNotebookAction, submitNotebookCommand } from './command';

const roisin = {
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};

describe('illustrated notebook command input', () => {
	it('seeds action stamps from the selected person', () => {
		expect(draftForNotebookAction('ask', roisin)).toBe('ask Roisin Connolly ');
		expect(draftForNotebookAction('observe', roisin)).toBe(
			'observe Roisin Connolly',
		);
		expect(draftForNotebookAction('talk', null)).toBe('talk to ');
	});

	it('uses an appearance label instead of an unintroduced canonical name', () => {
		expect(
			draftForNotebookAction('ask', {
				...roisin,
				name: 'a lean stranger with hard eyes',
				real_name: 'Sean Ruadh Kelly',
				occupation: 'labourer',
				introduced: false,
			}),
		).toBe('ask Lean stranger ');
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
