/**
 * E2E tests for user interactions: input submission, streaming, paused state.
 */

import { test, expect, installTauriMock, emitEvent, updateMockResponse } from './fixtures';
import { SNAPSHOTS, PALETTES, IRISH_HINTS } from './mock-data';

test.describe('Input field interactions', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('can type and submit text via Enter key', async ({ page }) => {
		const input = page.locator('.input-field');
		await input.fill('go to Howth');
		await input.press('Enter');

		// Input should be cleared after submission
		await expect(input).toHaveValue('');
	});

	test('input is disabled during streaming', async ({ page }) => {
		// Simulate loading state
		await emitEvent(page, 'loading', { active: true });
		await page.waitForTimeout(100);

		const input = page.locator('.input-field');
		await expect(input).toBeDisabled();

		// End loading
		await emitEvent(page, 'loading', { active: false });
		await page.waitForTimeout(100);
		await expect(input).toBeEnabled();
	});
});

test.describe('Streaming simulation', () => {
	test('stream tokens appear incrementally in chat', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// Start loading
		await emitEvent(page, 'loading', { active: true });
		await page.waitForTimeout(100);

		// Send tokens
		await emitEvent(page, 'stream-token', { token: 'Ah, ', turn_id: 1, source: 'Siobhan Murphy' });
		await page.waitForTimeout(50);
		await emitEvent(page, 'stream-token', { token: "you're ", turn_id: 1, source: 'Siobhan Murphy' });
		await page.waitForTimeout(50);
		await emitEvent(page, 'stream-token', { token: 'welcome!', turn_id: 1, source: 'Siobhan Murphy' });
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });
		await page.waitForTimeout(100);

		await expect(page.getByText("Ah, you're welcome!")).toBeVisible();

		// End stream
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
		await page.waitForTimeout(100);
	});

	test('keeps overlapping multi-npc streams attached to the right speaker', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await emitEvent(page, 'loading', { active: true });
		await page.waitForTimeout(100);

		await emitEvent(page, 'text-log', {
			id: 'msg-1',
			source: 'Siobhan Murphy',
			content: '',
			stream_turn_id: 11
		});
		await emitEvent(page, 'stream-token', {
			token: 'I heard the fair will be lively tonight ',
			turn_id: 11,
			source: 'Siobhan Murphy'
		});
		await page.waitForTimeout(80);
		await expect(page.locator('.bubble-row.npc').nth(0).locator('.label')).toHaveText('Siobhan Murphy');

		// Queue Padraig before Siobhan has finished animating.
		await emitEvent(page, 'text-log', {
			id: 'msg-2',
			source: 'Padraig Darcy',
			content: '',
			stream_turn_id: 12
		});
		await emitEvent(page, 'stream-token', {
			token: "If it is, I'll bring the cart before sunset.",
			turn_id: 12,
			source: 'Padraig Darcy'
		});

		await emitEvent(page, 'stream-token', {
			token: 'with music by the square.',
			turn_id: 11,
			source: 'Siobhan Murphy'
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 11 });
		await emitEvent(page, 'stream-turn-end', { turn_id: 12 });
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });

		await page.waitForTimeout(1500);

		const npcRows = page.locator('.bubble-row.npc');
		await expect(npcRows).toHaveCount(2);
		await expect(npcRows.nth(0).locator('.label')).toHaveText('Siobhan Murphy');
		await expect(npcRows.nth(0).locator('.content')).toContainText(
			'I heard the fair will be lively tonight with music by the square.'
		);
		await expect(npcRows.nth(1).locator('.label')).toHaveText('Padraig Darcy');
		await expect(npcRows.nth(1).locator('.content')).toContainText(
			"If it is, I'll bring the cart before sunset."
		);
	});
});

test.describe('Paused state', () => {
	test('shows paused indicator when game is paused', async ({ page }) => {
		const pausedSnapshot = { ...SNAPSHOTS.morning, paused: true };
		await installTauriMock(page, 'morning');

		// Override the snapshot with paused state
		await page.addInitScript(
			({ snapshot }) => {
				const responses = (window as unknown as Record<string, Record<string, unknown>>)
					.__TEST_MOCK_RESPONSES__;
				if (responses) responses['get_world_snapshot'] = snapshot;
			},
			{ snapshot: pausedSnapshot }
		);

		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await expect(page.getByText('Paused')).toBeVisible();
	});
});

test.describe('Festival badge', () => {
	test('shows festival badge when festival is active', async ({ page }) => {
		const festivalSnapshot = { ...SNAPSHOTS.morning, festival: 'Samhain' };
		await installTauriMock(page, 'morning');

		await page.addInitScript(
			({ snapshot }) => {
				const responses = (window as unknown as Record<string, Record<string, unknown>>)
					.__TEST_MOCK_RESPONSES__;
				if (responses) responses['get_world_snapshot'] = snapshot;
			},
			{ snapshot: festivalSnapshot }
		);

		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await expect(page.getByText('Samhain')).toBeVisible();
	});
});
