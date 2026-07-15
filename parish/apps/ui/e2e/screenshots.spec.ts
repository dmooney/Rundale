/**
 * Screenshot capture: generates docs/screenshots/gui-{time}.png
 * and creates visual regression baselines.
 *
 * Run: npx playwright test e2e/screenshots.spec.ts
 * Update baselines: npx playwright test e2e/screenshots.spec.ts --update-snapshots
 */

import {
	test,
	expect,
	installTauriMock,
	applyTheme,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';
import type { Page } from '@playwright/test';
import { PALETTES, SNAPSHOTS } from './mock-data';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TIMES_OF_DAY = ['morning', 'midday', 'dusk', 'night'] as const;
// Path is relative to apps/ui/e2e/screenshots.spec.ts → repo root → docs/screenshots/.
const SCREENSHOT_DIR = path.resolve(__dirname, '../../../docs/screenshots');

/**
 * Shared page setup for both the screenshot-generation and visual-regression
 * suites (TD-041): installs the Tauri mock for `time`, navigates, applies the
 * matching theme palette, and proves the Pixi notebook has rendered the
 * expected clock/weather state before capture.
 */
async function setupScreenshotPage(
	page: Page,
	time: (typeof TIMES_OF_DAY)[number],
): Promise<void> {
	await installTauriMock(page, time);
	await page.goto('/');
	await page.waitForLoadState('networkidle');

	await applyTheme(page, PALETTES[time]);
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
	await expect(
		page.getByTestId('illustrated-notebook-pixi-host').locator('canvas'),
	).toBeVisible();

	const timeControl = page.getByRole('button', { name: 'Open time details' });
	await expect(timeControl).toHaveCount(1);
	await timeControl.focus();
	await page.keyboard.press('Enter');
	const drawer = page.getByLabel('time drawer');
	await expect(drawer).toContainText(
		`${String(SNAPSHOTS[time].hour).padStart(2, '0')}:00`,
	);
	await expect(drawer).toContainText(SNAPSHOTS[time].time_label);
	await expect(drawer).toContainText(`Weather: ${SNAPSHOTS[time].weather}`);
	await page.getByRole('button', { name: 'Close notebook drawer' }).click();
	await expect(drawer).toHaveCount(0);
	await waitForTextureCompleteNotebookFrame(page);
}

test.describe('Screenshot generation', () => {
	for (const time of TIMES_OF_DAY) {
		test(`capture gui-${time}`, async ({ page }) => {
			await setupScreenshotPage(page, time);

			// Save to docs/screenshots/ for the project
			await page.screenshot({
				path: path.join(SCREENSHOT_DIR, `gui-${time}.png`),
				fullPage: false,
			});
		});
	}
});

test.describe('Visual regression baselines', () => {
	// Baselines are environment-specific (fonts, browser pixel rendering).
	// Skip in CI; run manually with `--update-snapshots` to refresh locally.
	test.skip(
		!!process.env.CI,
		'visual-regression baselines are environment-specific',
	);

	for (const time of TIMES_OF_DAY) {
		test(`visual-regression-${time}`, async ({ page }) => {
			await setupScreenshotPage(page, time);

			// Playwright visual comparison (stores baselines in snapshotDir)
			await expect(page).toHaveScreenshot(`gui-${time}.png`, {
				maxDiffPixelRatio: 0.02,
			});
		});
	}
});
