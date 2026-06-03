/**
 * Regression: the bug-report description must not leak global hotkeys.
 *
 * The global "M" key toggles the full map, but only when focus is not in a
 * text field. The guard originally exempted INPUT and contentEditable but not
 * TEXTAREA, so typing "M" in the bug-report description (`#bug-description`,
 * a <textarea>) opened the map instead of inserting the character.
 */

import { test, expect, installTauriMock } from './fixtures';

test.describe('Bug report modal — hotkey isolation', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('typing "m" in the description does not open the map', async ({ page }) => {
		// Open the bug-report modal from the status-bar 🐛 button.
		const bugButton = page.locator('.bug-toggle');
		await expect(bugButton).toBeVisible();
		await bugButton.click();
		await expect(page.locator('[data-testid="bug-report-modal"]')).toBeVisible({ timeout: 10_000 });
		await expect(page.locator('[data-testid="full-map"]')).toBeHidden();

		// Type text containing "m"/"M" into the description textarea.
		const description = page.locator('#bug-description');
		await description.click();
		await page.keyboard.type('Map breaks when I press m');

		// The "M" hotkey must NOT have toggled the map, and every character
		// must have landed in the field.
		await expect(page.locator('[data-testid="full-map"]')).toBeHidden();
		await expect(description).toHaveValue('Map breaks when I press m');
	});

	test('"m" still toggles the map when not typing in a field', async ({ page }) => {
		await expect(page.locator('[data-testid="full-map"]')).toBeHidden();
		// Ensure focus is not in any input/textarea.
		await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
		await page.keyboard.press('m');
		await expect(page.locator('[data-testid="full-map"]')).toBeVisible();
	});
});
