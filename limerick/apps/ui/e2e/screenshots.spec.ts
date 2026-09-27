import { expect, test, installTauriMock, emitEvent } from './fixtures';

const TIMES = ['morning', 'midday', 'dusk', 'night'] as const;

async function renderCompletedReply(page: import('@playwright/test').Page) {
	for (let index = 0; index < 18; index += 1) {
		await emitEvent(page, 'text-log', {
			id: `visual-history-${index}`,
			source: 'system',
			content: `Earlier parish event ${index + 1}: the road, weather, and neighbours remain in view.`,
		});
	}
	await emitEvent(page, 'loading', { active: true });
	await emitEvent(page, 'stream-token', {
		token:
			'The potatoes need tending first, then we will mend the western gate before the evening rain.',
		turn_id: 1835,
		source: 'Siobhan Murphy',
	});
	await emitEvent(page, 'stream-turn-end', { turn_id: 1835 });
	await emitEvent(page, 'stream-end', { hints: [] });
	await expect(page.getByTestId('input-field')).toHaveAttribute(
		'aria-busy',
		'false',
		{ timeout: 15_000 },
	);
}

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

	for (const viewport of [
		{ name: 'desktop', width: 1440, height: 900 },
		{ name: 'mobile', width: 390, height: 844 },
	]) {
		test(`${viewport.name}-latest-reply-visible`, async ({ page }) => {
			await page.setViewportSize(viewport);
			await installTauriMock(page, 'morning');
			await page.goto('/');
			await expect(page.getByTestId('app-root')).toHaveAttribute(
				'data-controller-ready',
				'true',
			);
			await renderCompletedReply(page);
			await expect(page).toHaveScreenshot(
				`gui-${viewport.name}-latest-reply.png`,
				{ animations: 'disabled' },
			);
		});
	}
});
