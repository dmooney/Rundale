import { expect, installTauriMock, test } from './fixtures';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/notebook-pixi-interactions',
);

async function setupNotebookPage(page: import('@playwright/test').Page) {
	await installTauriMock(page, 'morning');
	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await expect(
		page.locator('[data-testid="illustrated-notebook-game"]'),
	).toBeVisible();
	await expect(
		page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
	).toBeVisible();
	await expect(page.locator('.input-wrapper')).toHaveCount(0);
	await expect(page.locator('.input-form')).toHaveCount(0);
	await expect(page.locator('[data-testid="chat-panel"]')).toHaveCount(0);
	await expect(
		page.getByRole('button', { name: 'Ask action stamp' }),
	).toHaveCount(1);
}

test.describe('illustrated notebook interactions', () => {
	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('desktop Pixi hit targets and keyboard routing stay notebook-native', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);

		const input = page.getByLabel('Player intent');

		await page.mouse.click(630, 758);
		await expect(input).toHaveValue(/ask /);
		await expect(input).toBeFocused();

		await page.getByRole('button', { name: 'Open time details' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('time drawer')).toBeVisible();
		await expect(page.getByText('Clock')).toBeVisible();

		await page.getByRole('button', { name: 'Open parish map' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.locator('[data-testid="full-map"]')).toBeVisible();

		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop.png'),
			fullPage: false,
		});
	});

	test('mobile viewport keeps notebook controls and old chrome absent', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);

		await page.getByRole('button', { name: 'Ask action stamp' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('Player intent')).toHaveValue(/ask /);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile.png'),
			fullPage: false,
		});
	});
});
