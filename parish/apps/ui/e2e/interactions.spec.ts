/**
 * E2E tests for user interactions: input submission, streaming, paused state.
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import type { Locator, Page } from '@playwright/test';
import { SNAPSHOTS, IRISH_HINTS } from './mock-data';

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';

async function waitForNotebook(page: Page): Promise<void> {
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
	await expect(page.locator(PIXI_CANVAS)).toBeVisible();
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);
}

async function activateNotebookControl(
	page: Page,
	name: string,
): Promise<void> {
	const control = page.getByRole('button', { name, exact: true });
	await expect(control).toHaveCount(1);
	await expect(control).toBeEnabled();
	await control.focus();
	await expect(control).toBeFocused();
	await page.keyboard.press('Enter');
}

async function openJournal(page: Page): Promise<Locator> {
	await activateNotebookControl(page, 'Open Journal notebook tab');
	const journal = page.getByRole('dialog', {
		name: 'Parish Journal',
		exact: true,
	});
	await expect(journal).toBeVisible();
	await expect(journal).toHaveAttribute('data-surface', 'journal');
	await expect(journal.getByTestId('chat-panel')).toBeVisible();
	return journal;
}

async function openTimeAndWeather(page: Page): Promise<Locator> {
	await activateNotebookControl(page, 'Open time and weather');
	const sheet = page.getByRole('dialog', {
		name: 'Time & Weather',
		exact: true,
	});
	await expect(sheet).toBeVisible();
	return sheet;
}

async function openActiveIntents(page: Page): Promise<Locator> {
	await activateNotebookControl(page, 'Open active intents');
	const sheet = page.getByRole('dialog', {
		name: 'Active Intents',
		exact: true,
	});
	await expect(sheet).toBeVisible();
	return sheet;
}

async function installSubmitRecorder(page: Page): Promise<void> {
	await page.evaluate(() => {
		type Invoke = (
			command: string,
			args?: Record<string, unknown>,
		) => Promise<unknown>;
		const globals = window as unknown as Record<string, unknown>;
		const internals = globals.__TAURI_INTERNALS__ as { invoke: Invoke };
		const originalInvoke = internals.invoke.bind(internals);
		const submissions: string[] = [];
		globals.__TEST_SUBMIT_COMMANDS__ = submissions;
		internals.invoke = async (command, args) => {
			if (command === 'submit_input') {
				submissions.push(String(args?.text ?? ''));
			}
			return originalInvoke(command, args);
		};
	});
}

async function submittedCommands(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const globals = window as unknown as Record<string, unknown>;
		return (globals.__TEST_SUBMIT_COMMANDS__ as string[] | undefined) ?? [];
	});
}

function timeNoteRow(sheet: Locator, label: string): Locator {
	return sheet.locator('.ink-notes p').filter({ hasText: label });
}

test.describe('Input field interactions', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
	});

	test('can type and submit text via Enter key', async ({ page }) => {
		await installSubmitRecorder(page);
		const input = page.getByLabel('Player intent', { exact: true });
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'false');

		await input.fill('go to Howth');
		await input.press('Enter');

		// Input should be cleared after submission
		await expect(input).toHaveValue('');
		await expect.poll(() => submittedCommands(page)).toEqual(['go to Howth']);
	});

	// #1379: the input is always editable — no aria-disabled toggling.
	// During streaming the hidden native input exposes aria-busy so assistive
	// technology can report the in-flight reply, but the first keystroke can
	// still flush the stream to completion.
	test('input stays editable during streaming (flush-on-interaction, #1379)', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent', { exact: true });

		// Simulate a buffered reply so the next real keystroke must flush it.
		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'stream-token', {
			token: 'The whole reply appears when the player starts typing again.',
			turn_id: 1379,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1379 });

		// Field must stay natively editable; aria-busy is the stream signal.
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'true');

		await input.fill('next thought');
		await input.press('x');
		await expect(input).toHaveValue('next thoughtx');
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).not.toHaveAttribute('aria-disabled');

		const journal = await openJournal(page);
		await expect(journal).toContainText(
			'The whole reply appears when the player starts typing again.',
		);
	});

	// Regression for #991: the backend's handle_npc_conversation cancels
	// and re-spawns the loading animation per addressed NPC turn, so
	// `loading {active:false}` arrives mid-chain (between phase-1 NPC
	// turns, or between phase-1 and the autonomous follow-up chain).
	// #1379: the input is never aria-disabled; instead streamingActive is
	// reflected by aria-busy. The mid-chain loading=false must NOT clear that
	// state — only the terminal `stream-end` may.
	test('input stays in streaming state across mid-chain loading=false (#991)', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent', { exact: true });

		// Chain begins.
		await emitEvent(page, 'loading', { active: true });
		// #1379: always editable, never aria-disabled; aria-busy is the signal.
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'true');
		const intents = await openActiveIntents(page);
		await expect(intents).toContainText('pending');

		// NPC 1 streams a reply and the per-turn cancel fires.
		await emitEvent(page, 'stream-token', {
			token: 'Dia dhuit. ',
			turn_id: 1001,
			source: 'Padraig',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1001 });
		await emitEvent(page, 'loading', { active: false });

		// Input must remain busy even though loading=false has
		// arrived, because the chain has not yet emitted `stream-end`.
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(intents).toContainText('pending');

		// Autonomous follow-up turn (no fresh loading=true in this path).
		await emitEvent(page, 'stream-token', {
			token: 'Aye, indeed.',
			turn_id: 1002,
			source: 'Siobhan',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1002 });

		// Still streaming — chain still alive.
		await expect(input).toHaveAttribute('aria-busy', 'true');

		// Chain terminates.
		await emitEvent(page, 'stream-end', { hints: [] });

		// Only now does aria-busy clear while the field remains editable.
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(intents).toContainText('idle');
	});
});

test.describe('Streaming simulation', () => {
	test('stream tokens appear incrementally in chat', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
		const journal = await openJournal(page);
		const chatPanel = journal.getByTestId('chat-panel');

		// Start loading
		await emitEvent(page, 'loading', { active: true });

		// Send tokens
		await emitEvent(page, 'stream-token', {
			token: 'Ah, ',
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-token', {
			token: "you're ",
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-token', {
			token: 'welcome!',
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });

		await expect(chatPanel.getByText("Ah, you're welcome!")).toBeVisible();

		// End stream
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
	});

	test('keeps overlapping multi-npc streams attached to the right speaker', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
		const journal = await openJournal(page);
		const chatPanel = journal.getByTestId('chat-panel');

		await emitEvent(page, 'loading', { active: true });

		await emitEvent(page, 'text-log', {
			id: 'msg-1',
			source: 'Siobhan Murphy',
			content: '',
			stream_turn_id: 11,
		});
		await emitEvent(page, 'stream-token', {
			token: 'I heard the fair will be lively tonight ',
			turn_id: 11,
			source: 'Siobhan Murphy',
		});
		await expect(
			chatPanel.locator('.bubble-row.npc').nth(0).locator('.label'),
		).toHaveText('Siobhan Murphy');

		// Queue Padraig before Siobhan has finished animating.
		await emitEvent(page, 'text-log', {
			id: 'msg-2',
			source: 'Padraig Darcy',
			content: '',
			stream_turn_id: 12,
		});
		await emitEvent(page, 'stream-token', {
			token: "If it is, I'll bring the cart before sunset.",
			turn_id: 12,
			source: 'Padraig Darcy',
		});

		await emitEvent(page, 'stream-token', {
			token: 'with music by the square.',
			turn_id: 11,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 11 });
		await emitEvent(page, 'stream-turn-end', { turn_id: 12 });
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });

		const npcRows = chatPanel.locator('.bubble-row.npc');
		await expect(npcRows).toHaveCount(2);
		await expect(npcRows.nth(0).locator('.label')).toHaveText('Siobhan Murphy');
		await expect(npcRows.nth(0).locator('.content')).toContainText(
			'I heard the fair will be lively tonight with music by the square.',
		);
		await expect(npcRows.nth(1).locator('.label')).toHaveText('Padraig Darcy');
		await expect(npcRows.nth(1).locator('.content')).toContainText(
			"If it is, I'll bring the cart before sunset.",
		);
	});
});

test.describe('Paused state', () => {
	test('shows paused indicator when game is paused', async ({ page }) => {
		const pausedSnapshot = { ...SNAPSHOTS.morning, paused: true };
		await installTauriMock(page, 'morning', {
			snapshot: pausedSnapshot,
		});

		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
		const sheet = await openTimeAndWeather(page);

		await expect(timeNoteRow(sheet, 'Clock state')).toContainText('paused');
		await expect(timeNoteRow(sheet, 'Parish replies')).toContainText('ready');
	});

	test('shows inference-paused state without marking the clock paused', async ({
		page,
	}) => {
		const inferencePausedSnapshot = {
			...SNAPSHOTS.morning,
			inference_paused: true,
		};
		await installTauriMock(page, 'morning', {
			snapshot: inferencePausedSnapshot,
		});

		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
		const sheet = await openTimeAndWeather(page);

		await expect(timeNoteRow(sheet, 'Clock state')).toContainText('running');
		await expect(timeNoteRow(sheet, 'Parish replies')).toContainText('paused');
	});
});

test.describe('Festival badge', () => {
	test('shows festival badge when festival is active', async ({ page }) => {
		const festivalSnapshot = { ...SNAPSHOTS.morning, festival: 'Samhain' };
		await installTauriMock(page, 'morning', {
			snapshot: festivalSnapshot,
		});

		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
		const sheet = await openTimeAndWeather(page);

		await expect(timeNoteRow(sheet, 'Festival')).toContainText('Samhain');
	});
});
