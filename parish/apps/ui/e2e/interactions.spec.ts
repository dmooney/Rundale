/**
 * Player-input, streaming, and state contracts on the chat shell.
 */

import { expect, test, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';
import { IRISH_HINTS, SNAPSHOTS } from './mock-data';
import type { NpcInfo } from '../src/lib/types';

const STREAM_NPCS: NpcInfo[] = [
	{
		name: 'Siobhan Murphy',
		real_name: 'Siobhan Murphy',
		occupation: 'Farmer',
		mood: 'determined',
		introduced: true,
		mood_emoji: '•',
	},
	{
		name: 'Padraig Darcy',
		real_name: 'Padraig Darcy',
		occupation: 'Publican',
		mood: 'content',
		introduced: true,
		mood_emoji: '•',
	},
];

async function waitForChat(page: Page) {
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(
		page.getByRole('combobox', { name: 'Player input' }),
	).toBeEditable();
}

async function submissions(page: Page): Promise<string[]> {
	return page.evaluate(() =>
		(
			window as unknown as {
				__TEST_INVOKE_CALLS__: Array<{
					command: string;
					args?: { text?: string };
				}>;
			}
		).__TEST_INVOKE_CALLS__
			.filter((call) => call.command === 'submit_input')
			.map((call) => String(call.args?.text ?? '')),
	);
}

test.describe('Input and streaming', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await waitForChat(page);
	});

	test('can type and submit text via Enter', async ({ page }) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await input.fill('go to Howth');
		await input.press('Enter');
		await expect(input).toHaveText('');
		await expect.poll(() => submissions(page)).toEqual(['go to Howth']);
	});

	test('first interaction flushes an in-flight reply and remains editable', async ({
		page,
	}) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'stream-token', {
			token: 'The whole reply appears when the player starts typing again.',
			turn_id: 1379,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1379 });
		await expect(input).toHaveAttribute('aria-busy', 'true');

		await input.press('x');
		await expect(input).toHaveText('x');
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(page.getByTestId('chat-panel')).toContainText(
			'The whole reply appears when the player starts typing again.',
		);
	});

	test('mid-chain loading=false does not clear the streaming state', async ({
		page,
	}) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await emitEvent(page, 'loading', { active: true });
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await emitEvent(page, 'stream-token', {
			token: 'Dia dhuit.',
			turn_id: 1001,
			source: 'Padraig',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1001 });
		await emitEvent(page, 'loading', { active: false });
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await emitEvent(page, 'stream-end', { hints: [] });
		await expect(input).toHaveAttribute('aria-busy', 'false');
	});

	test('stream tokens appear incrementally in the visible transcript', async ({
		page,
	}) => {
		await emitEvent(page, 'loading', { active: true });
		for (const token of ['Ah, ', "you're ", 'welcome!']) {
			await emitEvent(page, 'stream-token', {
				token,
				turn_id: 1,
				source: 'Siobhan Murphy',
			});
		}
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });
		await expect(page.getByTestId('chat-panel')).toContainText(
			"Ah, you're welcome!",
		);
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
	});

	test('overlapping NPC turns remain attached to their speakers', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning', { npcs: STREAM_NPCS });
		await page.goto('/');
		await waitForChat(page);
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
		const chat = page.getByTestId('chat-panel');
		const siobhanRow = chat
			.locator('.bubble-row.npc')
			.filter({ hasText: 'Siobhan Murphy' });
		await expect(siobhanRow).toContainText(
			'I heard the fair will be lively tonight',
		);

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
		const padraigRow = chat
			.locator('.bubble-row.npc')
			.filter({ hasText: 'Padraig Darcy' });
		await expect(siobhanRow.locator('.label')).toHaveText('Siobhan Murphy');
		await expect(siobhanRow).toContainText(
			'I heard the fair will be lively tonight with music by the square.',
		);
		await expect(padraigRow.locator('.label')).toHaveText('Padraig Darcy');
		await expect(padraigRow).toContainText(
			"If it is, I'll bring the cart before sunset.",
		);
	});
});

test.describe('World-state chrome', () => {
	test('shows a paused indicator', async ({ page }) => {
		await installTauriMock(page, 'morning', {
			snapshot: { ...SNAPSHOTS.morning, paused: true },
		});
		await page.goto('/');
		await waitForChat(page);
		await expect(page.getByTestId('status-bar')).toContainText('Paused');
	});

	test('does not label the clock paused for inference-only pauses', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning', {
			snapshot: { ...SNAPSHOTS.morning, inference_paused: true },
		});
		await page.goto('/');
		await waitForChat(page);
		await expect(page.getByTestId('status-bar')).not.toContainText('Paused');
	});

	test('shows an active festival', async ({ page }) => {
		await installTauriMock(page, 'morning', {
			snapshot: { ...SNAPSHOTS.morning, festival: 'Samhain' },
		});
		await page.goto('/');
		await waitForChat(page);
		await expect(page.getByTestId('status-bar')).toContainText('Samhain');
	});
});
