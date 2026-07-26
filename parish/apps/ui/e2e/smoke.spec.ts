import { expect, test } from './fixtures';

test.describe('Smoke tests', () => {
	test('page loads with game state', async ({ parishPage: page }) => {
		await expect(page.getByTestId('chat-game-shell')).toBeVisible();
		await expect(page.getByTestId('status-bar')).toContainText(
			'Baile Átha Cliath',
		);
		await expect(page.getByTestId('scene-header')).toBeVisible();
	});

	test('player can type a command', async ({ parishPage: page }) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await input.fill('look around');
		await input.press('Enter');
		await expect(input).toHaveText('');
		await expect
			.poll(() =>
				page.evaluate(() =>
					(
						window as unknown as {
							__TEST_INVOKE_CALLS__: Array<{ command: string }>;
						}
					).__TEST_INVOKE_CALLS__.some(
						(call) => call.command === 'submit_input',
					),
				),
			)
			.toBe(true);
	});

	test('player can move to an adjacent location', async ({
		parishPage: page,
	}) => {
		await page.getByRole('button', { name: /^Travel to Binn Éadair/ }).click();
		await expect
			.poll(() =>
				page.evaluate(
					() =>
						(
							window as unknown as {
								__TEST_INVOKE_CALLS__: Array<{
									command: string;
									args?: { text?: string };
								}>;
							}
						).__TEST_INVOKE_CALLS__.find(
							(call) => call.command === 'submit_input',
						)?.args?.text,
				),
			)
			.toBe('go to Binn Éadair');
	});

	test('API endpoints return valid JSON', async ({ request }) => {
		for (const endpoint of [
			'/api/world-snapshot',
			'/api/map',
			'/api/npcs-here',
			'/api/theme',
			'/api/ui-config',
		]) {
			const response = await request.get(endpoint);
			expect(response.ok()).toBe(true);
			expect(await response.json()).toBeTruthy();
		}
	});

	test('secondary states remain screenshot-safe', async ({
		parishPage: page,
	}) => {
		await page.keyboard.press('F5');
		await expect(page.getByTestId('surface-save')).toBeVisible();
		const screenshot = await page.screenshot();
		expect(screenshot.byteLength).toBeGreaterThan(10_000);
	});
});
