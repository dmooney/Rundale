/**
 * Secondary player surfaces and editor navigation on the chat shell.
 */

import { expect, test, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';

async function waitForChat(page: Page) {
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
}

test.describe('Coordinated surfaces', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await waitForChat(page);
	});

	test('debug opens on F12, navigates tabs, closes, and reopens', async ({
		page,
	}, testInfo) => {
		await page.keyboard.press('F12');
		const debug = page.getByTestId('surface-debug');
		await expect(debug).toBeVisible();
		const tabs = debug.locator('.tab-bar button');
		await expect(tabs).toHaveCount(8);
		await tabs.nth(7).click();
		await expect(tabs.nth(7)).toHaveClass(/active/);
		await expect(debug).toContainText('gemini-3.6-flash');
		await expect(debug).toContainText('Session: 1 calls');
		await expect(debug).toContainText('cache 88.9%');
		await expect(debug).toContainText('estimated cost $0.00200');
		await debug.getByRole('button', { name: /#42/ }).click();
		await expect(debug).toContainText('google-interactions-v1');
		await expect(debug).toContainText('Provider interaction: int_test');
		if (process.env.PARISH_CAPTURE_INFERENCE_DEBUG) {
			await testInfo.attach('inference-debug-desktop', {
				body: await debug.screenshot(),
				contentType: 'image/png',
			});
		}

		await page.setViewportSize({ width: 390, height: 844 });
		await expect(debug).toBeVisible();
		const mobileBox = await debug.boundingBox();
		expect(mobileBox).not.toBeNull();
		expect(mobileBox!.x).toBeGreaterThanOrEqual(0);
		expect(mobileBox!.y).toBeGreaterThanOrEqual(0);
		expect(mobileBox!.x + mobileBox!.width).toBeLessThanOrEqual(390);
		expect(mobileBox!.y + mobileBox!.height).toBeLessThanOrEqual(844);
		if (process.env.PARISH_CAPTURE_INFERENCE_DEBUG) {
			await testInfo.attach('inference-debug-mobile', {
				body: await debug.screenshot(),
				contentType: 'image/png',
			});
		}
		await page.keyboard.press('Escape');
		await expect(debug).toHaveCount(0);
		await page.keyboard.press('F12');
		await expect(debug).toBeVisible();
	});

	test('save/load opens on F5, exposes Ledgers, and closes with Escape', async ({
		page,
	}) => {
		await page.keyboard.press('F5');
		const ledger = page.getByRole('dialog', { name: 'The Parish Ledger' });
		await expect(ledger).toBeVisible();
		await ledger.getByRole('button', { name: 'Ledgers' }).click();
		await expect(ledger).toContainText('Ledger');
		await page.keyboard.press('Escape');
		await expect(ledger).toContainText('The Parish Ledger');
		await page.keyboard.press('Escape');
		await expect(ledger).toHaveCount(0);
	});

	test('full map, shortcuts, mod, and bug report each have one destination', async ({
		page,
	}) => {
		await page.evaluate(() =>
			window.dispatchEvent(new KeyboardEvent('keydown', { key: 'm' })),
		);
		await expect(page.getByTestId('surface-map')).toBeVisible();
		await page.keyboard.press('Escape');

		await page.evaluate(() => (document.activeElement as HTMLElement)?.blur());
		await page.evaluate(() =>
			window.dispatchEvent(new KeyboardEvent('keydown', { key: '?' })),
		);
		await expect(
			page.getByRole('dialog', { name: 'Keyboard shortcuts' }),
		).toBeVisible();
		await page.getByRole('button', { name: 'Close shortcuts' }).click();

		await page.getByRole('button', { name: 'Developer tools menu' }).click();
		await page.getByRole('menuitem', { name: 'Switch active mod' }).click();
		await expect(
			page.getByRole('dialog', { name: 'Select mod' }),
		).toBeVisible();
		await page.getByRole('button', { name: 'Close' }).click();

		await page.getByRole('button', { name: 'Developer tools menu' }).click();
		await page.getByRole('menuitem', { name: 'Report a bug' }).click();
		await expect(page.getByTestId('bug-report-modal')).toBeVisible();
		await expect(page.getByRole('dialog')).toHaveCount(1);
	});

	test('sidebar keeps language hints visible', async ({ page }) => {
		const sidebar = page.getByTestId('sidebar');
		await expect(sidebar).toContainText('Focail (Irish Words)');
		await expect(sidebar).toContainText('[EE-fa]');
	});

	test('reaction bar and picker remain available in chat', async ({ page }) => {
		await emitEvent(page, 'text-log', {
			id: 'npc-reaction-target',
			source: 'Séamas Ó Briain',
			content: 'A quiet word from the square.',
		});
		await emitEvent(page, 'npc-reaction', {
			message_id: 'npc-reaction-target',
			emoji: '👍',
			source: 'Padraig Darcy',
		});
		const row = page
			.getByTestId('chat-panel')
			.locator('.bubble-row')
			.filter({ hasText: 'A quiet word from the square.' });
		await expect(row.getByTestId('reaction-bar')).toBeVisible();
		await row
			.getByRole('group', {
				name: 'NPC message — press Enter or Tab into the reaction picker',
			})
			.focus();
		await expect(row.getByTestId('reaction-picker')).toBeVisible();
	});
});

test.describe('Editor return', () => {
	test('opens the editor, switches tabs, and returns to chat', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await waitForChat(page);
		await page.getByRole('button', { name: 'Developer tools menu' }).click();
		await page.getByRole('menuitem', { name: 'Designer' }).click();
		await expect(page).toHaveURL(/\/editor/);
		await expect(page.getByTestId('editor-page')).toBeVisible();
		const tabs = page.locator('.editor-page .tab-bar button');
		await expect(tabs).toHaveCount(5);
		await tabs.nth(1).click();
		await expect(tabs.nth(1)).toHaveClass(/active/);
		await page.getByRole('link', { name: 'Game' }).click();
		await expect(page).not.toHaveURL(/\/editor/);
		await expect(page.getByTestId('chat-game-shell')).toBeVisible();
	});
});
