import { expect, test } from '@playwright/test';

test.describe('Real browser + parish-server acceptance', () => {
	test.beforeEach(async ({ page }) => {
		await page.goto('/');
		await page.evaluate(async () => {
			const response = await fetch('/api/new-game', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: '{}',
			});
			if (!response.ok) throw new Error(`new-game failed: ${response.status}`);
		});
		await page.reload();
		await expect(page.getByTestId('app-root')).toHaveAttribute(
			'data-controller-ready',
			'true',
		);
	});

	test('one browser session loads, moves, and stays aligned with engine state', async ({
		page,
	}, testInfo) => {
		expect(
			await page.evaluate(
				() => '__TAURI_INTERNALS__' in (window as unknown as object),
			),
		).toBe(false);
		await expect(page.getByTestId('status-bar')).toContainText('Kilteevan');

		const input = page.getByRole('combobox', { name: 'Player input' });
		await input.fill('go to the crossroads');
		await input.press('Enter');

		await expect
			.poll(() =>
				page.evaluate(async () => {
					const response = await fetch('/api/world-snapshot');
					if (!response.ok) return `HTTP ${response.status}`;
					const state = (await response.json()) as {
						location_name?: string;
					};
					return state.location_name ?? '';
				}),
			)
			.toContain('Crossroads');
		await expect(page.getByTestId('status-bar')).toContainText('Crossroads');

		const screenshot = await page.screenshot({ fullPage: true });
		expect(screenshot.byteLength).toBeGreaterThan(10_000);
		await testInfo.attach('real-server-after-movement', {
			body: screenshot,
			contentType: 'image/png',
		});
	});

	test('read APIs return JSON in the same browser session', async ({
		page,
	}) => {
		for (const endpoint of [
			'/api/world-snapshot',
			'/api/map',
			'/api/npcs-here',
			'/api/theme',
			'/api/ui-config',
		]) {
			const result = await page.evaluate(async (path) => {
				const response = await fetch(path);
				return { ok: response.ok, body: await response.json() };
			}, endpoint);
			expect(result.ok, endpoint).toBe(true);
			expect(result.body, endpoint).toBeTruthy();
		}
	});
});
