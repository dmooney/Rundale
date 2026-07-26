import { expect, test, installTauriMock } from './fixtures';

const TIMES = ['morning', 'midday', 'dusk', 'night'] as const;

test.describe('Chat-shell visual baselines', () => {
	for (const time of TIMES) {
		test(`visual-regression-${time}`, async ({ page }) => {
			await page.setViewportSize({ width: 1440, height: 900 });
			await installTauriMock(page, time);
			await page.goto('/');
			await expect(page.getByTestId('app-root')).toHaveAttribute(
				'data-controller-ready',
				'true',
			);
			await expect(
				page.getByTestId('scene-header').locator('img'),
			).toBeVisible();
			await expect(page.getByTestId('chat-panel')).toBeVisible();
			await expect(page.getByTestId('input-field')).toBeVisible();
			await expect(page).toHaveScreenshot(`gui-${time}.png`, {
				animations: 'disabled',
			});
		});
	}

	test('mobile-chat-shell', async ({ page }) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await expect(page.getByTestId('input-field')).toBeVisible();
		await expect(page).toHaveScreenshot('gui-mobile.png', {
			animations: 'disabled',
		});
	});
});
