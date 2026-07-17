/**
 * E2E tests for user interactions: input submission, streaming, paused state.
 */

import type { Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { test, expect, installTauriMock, emitEvent } from './fixtures';
import { SNAPSHOTS, IRISH_HINTS } from './mock-data';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/1712-notebook-journal-e2e',
);

async function openNotebookDrawer(page: Page, name: 'journal' | 'intents') {
	const triggerName =
		name === 'journal' ? 'Open Journal notebook tab' : 'Open active intents';
	const trigger = page.getByRole('button', { name: triggerName });
	await expect(trigger).toBeVisible();
	await trigger.focus();
	await page.keyboard.press('Enter');
	const drawer = page.getByLabel(`${name} drawer`);
	await expect(drawer).toBeVisible();
	return drawer;
}

async function installSubmitRecorder(page: Page) {
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

test.describe('Input field interactions', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await expect(
			page.locator('[data-testid="illustrated-notebook-game"]'),
		).toBeVisible();
	});

	test('can type and submit text via Enter key', async ({ page }) => {
		await installSubmitRecorder(page);
		const input = page.getByLabel('Player intent');
		await input.fill('go to Howth');
		await input.press('Enter');

		await expect(input).toHaveValue('');
		await expect.poll(() => submittedCommands(page)).toEqual(['go to Howth']);
	});

	// #1379: the notebook input remains an enabled native field while aria-busy
	// communicates the in-flight reply. The first keystroke flushes that reply
	// and is then accepted into the draft.
	test('input stays editable during streaming (flush-on-interaction, #1379)', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent');

		// Give the stream manager a real buffered turn. The native notebook input
		// advertises that the reply is busy, but remains an actual editable input
		// so the first keystroke can flush the reply.
		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'stream-token', {
			token: 'The whole reply appears when the player starts typing again.',
			turn_id: 1379,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1379 });

		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(input).not.toHaveAttribute('disabled', '');
		await input.fill('next thought');
		await input.press('x');
		await expect(input).toHaveValue('next thoughtx');
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'false');

		const journal = await openNotebookDrawer(page, 'journal');
		await expect(journal).toContainText(
			'The whole reply appears when the player starts typing again.',
		);
	});

	// Regression for #991: the backend's handle_npc_conversation cancels
	// and re-spawns the loading animation per addressed NPC turn, so
	// `loading {active:false}` arrives mid-chain (between phase-1 NPC
	// turns, or between phase-1 and the autonomous follow-up chain).
	// The mid-chain loading=false must NOT clear the notebook busy state — only
	// the terminal `stream-end` may return the current reply to idle.
	test('input stays in streaming state across mid-chain loading=false (#991)', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent');
		const intents = await openNotebookDrawer(page, 'intents');

		// Chain begins.
		await emitEvent(page, 'loading', { active: true });
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(input).not.toHaveAttribute('disabled', '');
		await expect(intents).toContainText('Parish reply: pending');

		// NPC 1 streams a reply and the per-turn cancel fires.
		await emitEvent(page, 'stream-token', {
			token: 'Dia dhuit. ',
			turn_id: 1001,
			source: 'Padraig',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1001 });
		await emitEvent(page, 'loading', { active: false });

		// The notebook must remain busy even though loading=false has arrived,
		// because the chain has not yet emitted `stream-end`.
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(input).not.toHaveAttribute('disabled', '');
		await expect(intents).toContainText('Parish reply: pending');

		fs.mkdirSync(PROOF_DIR, { recursive: true });
		await page.screenshot({
			path: path.join(PROOF_DIR, 'mid-chain-input-streaming.png'),
			fullPage: false,
		});

		// Autonomous follow-up turn (no fresh loading=true in this path).
		await emitEvent(page, 'stream-token', {
			token: 'Aye, indeed.',
			turn_id: 1002,
			source: 'Siobhan',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1002 });

		// Still busy — the chain is still alive.
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(intents).toContainText('Parish reply: pending');

		// Chain terminates.
		await emitEvent(page, 'stream-end', { hints: [] });

		// Only now does the notebook return to idle.
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(intents).toContainText('Parish reply: idle');
	});
});

test.describe('Streaming simulation', () => {
	test('stream tokens appear incrementally in the Journal', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		const journal = await openNotebookDrawer(page, 'journal');

		// Start loading
		await emitEvent(page, 'loading', { active: true });

		// Send tokens and observe each increment through the notebook Journal.
		await emitEvent(page, 'stream-token', {
			token: 'Ah, ',
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		const reply = journal.locator('p').filter({ hasText: 'Siobhan Murphy' });
		await expect(reply).toContainText('Ah,');
		await emitEvent(page, 'stream-token', {
			token: "you're ",
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await expect(reply).toContainText("Ah, you're");
		await emitEvent(page, 'stream-token', {
			token: 'welcome!',
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });

		await expect(reply).toContainText("Ah, you're welcome!");

		// End stream
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
	});

	test('keeps overlapping multi-npc streams attached to the right speaker', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		const journal = await openNotebookDrawer(page, 'journal');

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
		const siobhan = journal.locator('p').filter({ hasText: 'Siobhan Murphy' });
		await expect(siobhan).toHaveCount(1);
		await expect(siobhan).toContainText(
			'I heard the fair will be lively tonight',
		);

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

		const padraig = journal.locator('p').filter({ hasText: 'Padraig Darcy' });
		await expect(siobhan).toContainText(
			'I heard the fair will be lively tonight with music by the square.',
		);
		await expect(padraig).toContainText(
			"If it is, I'll bring the cart before sunset.",
		);
		const lines = await journal.locator('p').allTextContents();
		const siobhanIdx = lines.findIndex((line) =>
			line.includes('Siobhan Murphy'),
		);
		const padraigIdx = lines.findIndex((line) =>
			line.includes('Padraig Darcy'),
		);
		expect(siobhanIdx).toBeGreaterThan(-1);
		expect(padraigIdx).toBeGreaterThan(-1);
		expect(siobhanIdx).toBeLessThan(padraigIdx);
	});
});

test.describe('Paused state', () => {
	test('shows paused indicator when game is paused', async ({ page }) => {
		test.fixme(
			true,
			'#1715 restores a visible and accessible paused-state contract to the illustrated notebook',
		);
		const pausedSnapshot = { ...SNAPSHOTS.morning, paused: true };
		await installTauriMock(page, 'morning');

		// Override the snapshot with paused state
		await page.addInitScript(
			({ snapshot }) => {
				const responses = (
					window as unknown as Record<string, Record<string, unknown>>
				).__TEST_MOCK_RESPONSES__;
				if (responses) responses['get_world_snapshot'] = snapshot;
			},
			{ snapshot: pausedSnapshot },
		);

		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await expect(page.getByText('Paused')).toBeVisible();
	});
});

test.describe('Festival badge', () => {
	test('shows festival badge when festival is active', async ({ page }) => {
		test.fixme(
			true,
			'#1716 restores the active festival to the illustrated notebook world-state surface',
		);
		const festivalSnapshot = { ...SNAPSHOTS.morning, festival: 'Samhain' };
		await installTauriMock(page, 'morning');

		await page.addInitScript(
			({ snapshot }) => {
				const responses = (
					window as unknown as Record<string, Record<string, unknown>>
				).__TEST_MOCK_RESPONSES__;
				if (responses) responses['get_world_snapshot'] = snapshot;
			},
			{ snapshot: festivalSnapshot },
		);

		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await expect(page.getByText('Samhain')).toBeVisible();
	});
});
