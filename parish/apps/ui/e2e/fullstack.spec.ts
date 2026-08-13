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

	test('synchronous command API classifies unknown slash input as system output', async ({
		page,
	}) => {
		const result = await page.evaluate(async () => {
			const response = await fetch('/api/command', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ text: '/not-a-command', timeoutMs: 2_000 }),
			});
			return (await response.json()) as {
				kind: string;
				lines: Array<{ text: string }>;
			};
		});
		expect(result.kind).toBe('system');
		expect(result.lines.map((line) => line.text).join('\n')).toContain(
			'Unknown system command',
		);
	});

	test('advertised slash commands execute as system commands and named load restores the branch', async ({
		page,
	}) => {
		const input = page.getByRole('combobox', { name: 'Player input' });

		await input.fill('/fork alternate');
		await input.press('Enter');
		await expect(
			page.getByText("Created new branch 'alternate'."),
		).toBeVisible();

		await input.fill('go to the crossroads');
		await input.press('Enter');
		await expect(page.getByTestId('status-bar')).toContainText('Crossroads');

		await input.fill('/load main');
		await input.press('Enter');
		await expect(page.getByTestId('status-bar')).toContainText('Kilteevan');
		await expect(page.getByText(/Loaded .*branch: main/)).toBeVisible();

		await input.fill('/irish');
		await input.press('Enter');
		await expect(page.getByText(/Irish words panel/i)).toBeVisible();

		const saveBeforeNewGame = await page.evaluate(async () => {
			const response = await fetch('/api/save-state');
			return (await response.json()) as { filename?: string };
		});
		await input.fill('/new-game');
		await input.press('Enter');
		await expect(page.getByTestId('status-bar')).toContainText('Kilteevan');
		await expect
			.poll(() =>
				page.evaluate(async () => {
					const response = await fetch('/api/save-state');
					return ((await response.json()) as { filename?: string }).filename;
				}),
			)
			.not.toBe(saveBeforeNewGame.filename);
	});

	test('nearby-person labels mirror authoritative server identity state', async ({
		page,
	}) => {
		const npcs = await page.evaluate(async () => {
			const response = await fetch('/api/npcs-here');
			if (!response.ok) throw new Error(`npcs-here failed: ${response.status}`);
			return (await response.json()) as Array<{
				name: string;
				real_name: string;
				occupation: string;
				introduced: boolean;
			}>;
		});
		expect(npcs.length).toBeGreaterThan(0);

		const present = page.getByTestId('npcs-present');
		await expect(present).toBeVisible();
		for (const npc of npcs) {
			await expect(present.getByText(npc.name, { exact: true })).toBeVisible();
			if (npc.introduced && npc.occupation) {
				await expect(
					present.getByText(npc.occupation, { exact: true }),
				).toBeVisible();
			} else {
				expect(npc.name).not.toBe(npc.real_name);
				await expect(
					present.getByText(npc.real_name, { exact: true }),
				).toHaveCount(0);
			}
		}
	});

	test('weather diagnostics render the canonical 1820 check time', async ({
		page,
	}, testInfo) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await input.fill('/weather overcast');
		await input.press('Enter');

		await expect
			.poll(() =>
				page.evaluate(async () => {
					const response = await fetch('/api/debug-snapshot');
					if (!response.ok) return null;
					return (await response.json()).weather as {
						last_check_at?: string | null;
						last_check_hour?: number;
					};
				}),
			)
			.toMatchObject({
				last_check_at: expect.stringMatching(/^\d{2}:\d{2} 1820-03-20$/),
			});
		const snapshot = await page.evaluate(async () => {
			const response = await fetch('/api/debug-snapshot');
			return (await response.json()).weather as {
				last_check_at: string;
				last_check_hour?: number;
			};
		});
		expect(snapshot.last_check_hour).toBeUndefined();

		await page.getByRole('button', { name: 'Developer tools menu' }).click();
		await page
			.getByRole('menuitemcheckbox', { name: 'Toggle debug panel' })
			.click();
		await page.getByRole('button', { name: 'Weather', exact: true }).click();
		const dialog = page.getByRole('dialog', { name: 'Debug records' });
		await expect(dialog).toContainText(
			`Last checked: ${snapshot.last_check_at}`,
		);
		await expect(dialog).not.toContainText('Last check hour');

		await testInfo.attach('weather-debug-canonical-check-time', {
			body: await page.screenshot({ fullPage: true }),
			contentType: 'image/png',
		});
	});
});
