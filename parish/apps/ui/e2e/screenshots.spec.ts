/**
 * Screenshot capture: generates docs/screenshots/gui-{time}.png
 * and creates visual regression baselines.
 *
 * Run: npx playwright test e2e/screenshots.spec.ts
 * Update baselines: npx playwright test e2e/screenshots.spec.ts --update-snapshots
 */

import { test, expect, installTauriMock, applyTheme, emitEvent, addTextLog } from './fixtures';
import { SNAPSHOTS, PALETTES, TEXT_LOG } from './mock-data';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TIMES_OF_DAY = ['morning', 'midday', 'dusk', 'night'] as const;
// Path is relative to apps/ui/e2e/screenshots.spec.ts → repo root → docs/screenshots/.
const SCREENSHOT_DIR = path.resolve(__dirname, '../../../docs/screenshots');

test.describe('Screenshot generation', () => {
	for (const time of TIMES_OF_DAY) {
		test(`capture gui-${time}`, async ({ page }) => {
			await installTauriMock(page, time);
			await page.goto('/');
			await page.waitForLoadState('networkidle');

			// Apply the correct theme palette
			await applyTheme(page, PALETTES[time]);

			// Add some chat content for visual interest
			for (const entry of TEXT_LOG) {
				await addTextLog(page, entry);
			}

			// Wait for all renders to settle
			await page.waitForLoadState('networkidle');

			// Save to docs/screenshots/ for the project
			await page.screenshot({
				path: path.join(SCREENSHOT_DIR, `gui-${time}.png`),
				fullPage: false
			});
		});
	}
});

test.describe('Visual regression baselines', () => {
	// Baselines are environment-specific (fonts, browser pixel rendering).
	// Skip in CI; run manually with `--update-snapshots` to refresh locally.
	test.skip(!!process.env.CI, 'visual-regression baselines are environment-specific');

	for (const time of TIMES_OF_DAY) {
		test(`visual-regression-${time}`, async ({ page }) => {
			await installTauriMock(page, time);
			await page.goto('/');
			await page.waitForLoadState('networkidle');

			// Apply theme
			await applyTheme(page, PALETTES[time]);

			// Add chat content
			for (const entry of TEXT_LOG) {
				await addTextLog(page, entry);
			}

			await page.waitForLoadState('networkidle');

			// Playwright visual comparison (stores baselines in snapshotDir)
			await expect(page).toHaveScreenshot(`gui-${time}.png`, {
				maxDiffPixelRatio: 0.02
			});
		});
	}
});
