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
		const input = page.locator('[data-testid="input-field"]');
		await input.fill('go to Howth');
		await input.press('Enter');

		// Input should be cleared after submission
		await expect(input).toHaveText('');
	});

	test('input is disabled during streaming', async ({ page }) => {
		// Simulate loading state
		await emitEvent(page, 'loading', { active: true });

		const input = page.locator('[data-testid="input-field"]');
		await expect(input).toHaveAttribute('aria-disabled', 'true');

		// End loading
		await emitEvent(page, 'loading', { active: false });
		await expect(input).toHaveAttribute('aria-disabled', 'false');
	});

	// Regression for #991: the backend's handle_npc_conversation cancels
	// and re-spawns the loading animation per addressed NPC turn, so
	// `loading {active:false}` arrives mid-chain (between phase-1 NPC
	// turns, or between phase-1 and the autonomous follow-up chain).
	// The frontend must NOT re-enable the input field on that mid-chain
	// loading=false — only the chain's terminal `stream-end` may.
	test('input stays disabled across mid-chain loading=false (#991)', async ({ page }) => {
		const input = page.locator('[data-testid="input-field"]');

		// Chain begins.
		await emitEvent(page, 'loading', { active: true });
		await expect(input).toHaveAttribute('aria-disabled', 'true');

		// NPC 1 streams a reply and the per-turn cancel fires.
		await emitEvent(page, 'stream-token', { token: 'Dia dhuit. ', turn_id: 1001, source: 'Padraig' });
		await emitEvent(page, 'stream-turn-end', { turn_id: 1001 });
		await emitEvent(page, 'loading', { active: false });

		// Input must remain disabled even though loading=false has arrived,
		// because the chain has not yet emitted `stream-end`.
		await expect(input).toHaveAttribute('aria-disabled', 'true');

		// Capture the mid-chain state as proof for #991 (rule #10 screenshot tier).
		await page.screenshot({
			path: '../../../docs/proofs/991-streaming-active-chain-gap/screenshots/mid-chain-input-disabled.png',
			fullPage: false
		});

		// Autonomous follow-up turn (no fresh loading=true in this path).
		await emitEvent(page, 'stream-token', { token: 'Aye, indeed.', turn_id: 1002, source: 'Siobhan' });
		await emitEvent(page, 'stream-turn-end', { turn_id: 1002 });

		// Still disabled — chain still alive.
		await expect(input).toHaveAttribute('aria-disabled', 'true');

		// Chain terminates.
		await emitEvent(page, 'stream-end', { hints: [] });

		// Only now does the input re-enable.
		await expect(input).toHaveAttribute('aria-disabled', 'false');
	});
});

test.describe('Streaming simulation', () => {
	test('stream tokens appear incrementally in chat', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// Start loading
		await emitEvent(page, 'loading', { active: true });

		// Send tokens
		await emitEvent(page, 'stream-token', { token: 'Ah, ', turn_id: 1, source: 'Siobhan Murphy' });
		await emitEvent(page, 'stream-token', { token: "you're ", turn_id: 1, source: 'Siobhan Murphy' });
		await emitEvent(page, 'stream-token', { token: 'welcome!', turn_id: 1, source: 'Siobhan Murphy' });
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });

		await expect(page.getByText("Ah, you're welcome!")).toBeVisible();

		// End stream
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
	});

	test('keeps overlapping multi-npc streams attached to the right speaker', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await emitEvent(page, 'loading', { active: true });

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
