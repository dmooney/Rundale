import { test, expect, type Page } from '@playwright/test';
import {
	installTileRouteMock,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';

async function waitForNotebook(page: Page): Promise<void> {
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible({
		timeout: 10_000,
	});
	await expect(
		page.getByTestId('illustrated-notebook-pixi-host').locator('canvas'),
	).toBeVisible();
	await expect(
		page.getByRole('button', { name: 'Open Journal notebook tab' }),
	).toHaveCount(1);
}

async function activateNotebookControl(
	page: Page,
	name: string,
): Promise<void> {
	const control = page.getByRole('button', { name });
	await expect(control).toHaveCount(1);
	await control.focus();
	await page.keyboard.press('Enter');
}

async function submitIntent(page: Page, text: string): Promise<void> {
	const input = page.getByLabel('Player intent');
	await input.focus();
	await expect(input).toBeFocused();
	await page.keyboard.insertText(text);
	await expect(input).toHaveValue(text);
	await page.keyboard.press('Enter');
	await expect(input).toHaveValue('', { timeout: 30_000 });
}

test.describe('Parish Web UI', () => {
	test.beforeEach(async ({ page }) => {
		await installTileRouteMock(page);
	});

	test('page loads with game state', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		await expect(page.getByLabel('Player intent')).toHaveCount(1);
		await activateNotebookControl(page, 'Open time details');
		const timeDrawer = page.getByLabel('time drawer');
		await expect(timeDrawer).toContainText(
			/Morning|Midday|Afternoon|Dusk|Night|Dawn/,
		);
		await expect(timeDrawer).toContainText('Weather:');

		await page.getByRole('button', { name: 'Close notebook drawer' }).click();
		await activateNotebookControl(page, 'Open Journal notebook tab');
		await expect(page.getByLabel('journal drawer')).not.toBeEmpty();
	});

	test('player can submit a natural-language command', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		const snapshotResponse = await page.request.get('/api/world-snapshot');
		expect(snapshotResponse.ok()).toBeTruthy();
		const snapshot = (await snapshotResponse.json()) as {
			location_description: string;
		};

		await submitIntent(page, 'look');
		await activateNotebookControl(page, 'Open Journal notebook tab');
		const journal = page.getByLabel('journal drawer');
		await expect(
			journal.locator('.journal-lines p').filter({ hasText: 'player: look' }),
		).toHaveCount(1, { timeout: 30_000 });
		await expect(
			journal
				.locator('.journal-lines p')
				.filter({ hasText: snapshot.location_description }),
		).toHaveCount(2, { timeout: 30_000 });
	});

	test('player can move to a location', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		const mapResponse = await page.request.get('/api/map');
		expect(mapResponse.ok()).toBeTruthy();
		const mapData = (await mapResponse.json()) as {
			locations: Array<{ name: string; adjacent: boolean }>;
		};
		const destination = mapData.locations.find((location) => location.adjacent);
		expect(destination).toBeTruthy();
		if (!destination) return;

		await submitIntent(page, `go to ${destination.name}`);
		await expect
			.poll(
				async () => {
					const response = await page.request.get('/api/world-snapshot');
					const snapshot = (await response.json()) as { location_name: string };
					return snapshot.location_name;
				},
				{ timeout: 30_000 },
			)
			.toBe(destination.name);

		await activateNotebookControl(page, 'Open Journal notebook tab');
		await expect(page.getByLabel('journal drawer')).toContainText(
			destination.name,
			{ timeout: 30_000 },
		);
	});

	test('API endpoints return valid JSON', async ({ request }) => {
		// World snapshot
		const snap = await request.get('/api/world-snapshot');
		expect(snap.ok()).toBeTruthy();
		const snapData = await snap.json();
		expect(snapData.location_name).toBeTruthy();
		expect(snapData.hour).toBeGreaterThanOrEqual(0);
		expect(snapData.hour).toBeLessThanOrEqual(23);

		// Map
		const map = await request.get('/api/map');
		expect(map.ok()).toBeTruthy();
		const mapData = await map.json();
		expect(mapData.player_location).toBeTruthy();
		expect(Array.isArray(mapData.locations)).toBeTruthy();

		// NPCs here
		const npcs = await request.get('/api/npcs-here');
		expect(npcs.ok()).toBeTruthy();
		const npcsData = await npcs.json();
		expect(Array.isArray(npcsData)).toBeTruthy();

		// Theme
		const theme = await request.get('/api/theme');
		expect(theme.ok()).toBeTruthy();
		const themeData = await theme.json();
		expect(themeData.bg).toMatch(/^#[0-9a-f]{6}$/);
	});

	test('screenshot at different states', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		await expect(page.locator('.app-shell')).toBeVisible();
		await waitForTextureCompleteNotebookFrame(page);
		await page.screenshot({
			path: 'e2e-results/initial-load.png',
			fullPage: false,
		});

		await submitIntent(page, '/status');
		await activateNotebookControl(page, 'Open Journal notebook tab');
		await expect(page.getByLabel('journal drawer')).toContainText('Location:', {
			timeout: 30_000,
		});
		await waitForTextureCompleteNotebookFrame(page);
		await page.screenshot({
			path: 'e2e-results/after-status.png',
			fullPage: false,
		});
	});
});
