import { test, expect } from '@playwright/test';
import { installTileRouteMock } from './fixtures';

test.describe('Parish Web UI', () => {
	test.beforeEach(async ({ page }) => {
		await installTileRouteMock(page);
	});

	test('page loads with game state', async ({ page }) => {
		await page.goto('/');

		const notebook = page.locator(
			'[data-testid="illustrated-notebook-game"]',
		);
		await expect(notebook).toBeVisible({ timeout: 10_000 });
		await expect(
			page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
		).toBeVisible();

		// The accessible chronicle mirrors the Pixi-rendered live text.
		const chronicle = page.getByLabel('Live chronicle');
		await expect(chronicle).toBeAttached();
		await expect(chronicle).not.toBeEmpty();

		// The native input and canvas controls back the illustrated surface.
		await expect(page.getByLabel('Player intent')).toBeEnabled();
		await expect(
			page.getByRole('button', { name: 'Open parish map' }),
		).toHaveCount(1);
		await expect(
			page.getByRole('button', { name: 'Open time details' }),
		).toHaveCount(1);

		await page.getByRole('button', { name: 'Open time details' }).focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('time drawer')).toContainText('Clock');
	});

	test('player can type a command', async ({ page }) => {
		await page.goto('/');

		// Wait for initial load
		await expect(
			page.locator('[data-testid="illustrated-notebook-game"]'),
		).toBeVisible({ timeout: 10_000 });

		// Type a look command
		const input = page.getByLabel('Player intent');
		const chronicle = page.getByLabel('Live chronicle');
		const initialLineCount = await chronicle.locator('p').count();
		await input.fill('look');
		await input.press('Enter');

		// A bare `look` is an action, and its canonical result must add a new
		// line to the notebook chronicle after the backend round-trip.
		await expect
			.poll(() => chronicle.locator('p').count(), { timeout: 30_000 })
			.toBeGreaterThan(initialLineCount);
		await expect(page.locator('.bubble-row.player')).toHaveCount(0);
	});

	test('player can move to a location', async ({ page }) => {
		await page.goto('/');
		await expect(
			page.locator('[data-testid="illustrated-notebook-game"]'),
		).toBeVisible({ timeout: 10_000 });

		const host = page.getByTestId('illustrated-notebook-pixi-host');
		await expect(host).toHaveAttribute('data-scene-location-id', '15');
		await expect(host).toHaveAttribute(
			'data-scene-plate',
			'/rundale/notebook-ui/scene-kilteevan-village.png',
		);

		const input = page.getByLabel('Player intent');
		await input.fill('go to The Crossroads');
		await input.press('Enter');

		// This is a real backend movement (not a mocked world-update). The
		// authoritative world snapshot must change the Pixi scene identity even
		// before its independently-refreshed map catches up.
		await expect(host).toHaveAttribute('data-scene-location-id', '1', {
			timeout: 30_000,
		});
		await expect(host).toHaveAttribute(
			'data-scene-plate',
			'/rundale/notebook-ui/scene-crossroads.png',
		);
	});

	test('API endpoints return valid JSON', async ({ request }) => {
		// World snapshot
		const snap = await request.get('/api/world-snapshot');
		expect(snap.ok()).toBeTruthy();
		const snapData = await snap.json();
		expect(snapData.location_id).toBeGreaterThan(0);
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
		await expect(
			page.locator('[data-testid="illustrated-notebook-game"]'),
		).toBeVisible({ timeout: 10_000 });

		// Wait for the app shell to be fully rendered before taking the screenshot.
		await expect(page.locator('.app-shell')).toBeVisible();
		await page.screenshot({
			path: 'e2e-results/initial-load.png',
			fullPage: true,
		});

		// After a command
		const input = page.getByLabel('Player intent');
		await input.fill('/status');
		await input.press('Enter');
		await expect(page.getByLabel('Live chronicle')).toContainText(
			'Location:',
			{ timeout: 5_000 },
		);
		await page.screenshot({
			path: 'e2e-results/after-status.png',
			fullPage: true,
		});
	});
});
