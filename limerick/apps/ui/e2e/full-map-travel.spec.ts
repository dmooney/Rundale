import { expect, test, installTauriMock } from './fixtures';
import type { Page, TestInfo } from '@playwright/test';

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

async function openFullMap(page: Page) {
	await page.getByTestId('app-root').waitFor();
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await page.getByTestId('status-bar').click();
	await page.keyboard.press('m');
	await expect(page.getByTestId('full-map')).toBeVisible();
}

async function attachMapScreenshot(
	page: Page,
	testInfo: TestInfo,
	name: string,
) {
	await testInfo.attach(name, {
		body: await page.getByTestId('full-map').screenshot(),
		contentType: 'image/png',
	});
}

test.describe('Full-map travel activation', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
	});

	test('one pointer click submits travel exactly once', async ({
		page,
	}, testInfo) => {
		await openFullMap(page);
		const destination = page.getByTestId('full-map').getByRole('button', {
			name: 'Travel to Binn Éadair',
		});
		await expect(destination).toBeVisible();
		await destination.click();
		await expect.poll(() => submissions(page)).toEqual(['go to Binn Éadair']);
		await attachMapScreenshot(page, testInfo, 'full-map-pointer-travel');
	});

	test('a focused destination activates with the keyboard on desktop and mobile', async ({
		page,
	}, testInfo) => {
		for (const viewport of [
			{ name: 'desktop', width: 1280, height: 800 },
			{ name: 'mobile', width: 390, height: 844 },
		]) {
			await page.setViewportSize(viewport);
			if (viewport.name === 'mobile') await page.reload();
			await openFullMap(page);
			const destination = page
				.getByTestId('full-map')
				.getByRole('button', { name: 'Travel to Deilginse' });
			await destination.focus();
			await expect(destination).toBeFocused();
			await attachMapScreenshot(
				page,
				testInfo,
				`full-map-keyboard-${viewport.name}`,
			);
			await page.keyboard.press('Enter');
			await expect.poll(() => submissions(page)).toEqual(['go to Deilginse']);
		}
	});
});
