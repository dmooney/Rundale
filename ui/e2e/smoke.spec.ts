import { test, expect } from '@playwright/test';

test.describe('Parish Web UI', () => {
	test('page loads with game state', async ({ page }) => {
		await page.goto('/');

		// Status bar should show a time-of-day label
		const statusBar = page.locator('[data-testid="status-bar"]');
		await expect(statusBar).toBeVisible({ timeout: 10_000 });
		await expect(statusBar).toContainText(/Morning|Midday|Afternoon|Dusk|Night|Dawn/);

		// Chat panel should have the initial location description
		const chatPanel = page.locator('[data-testid="chat-panel"]');
		await expect(chatPanel).toBeVisible();
		await expect(chatPanel).not.toBeEmpty();

		// Input field should be present
		const inputField = page.locator('[data-testid="input-field"] input');
		await expect(inputField).toBeVisible();

		// Map panel should render
		const mapPanel = page.locator('[data-testid="map-panel"]');
		await expect(mapPanel).toBeVisible();

		// Sidebar should render
		const sidebar = page.locator('[data-testid="sidebar"]');
		await expect(sidebar).toBeVisible();
	});

	test('player can type a command', async ({ page }) => {
		await page.goto('/');

		// Wait for initial load
		await expect(page.locator('[data-testid="status-bar"]')).toBeVisible({ timeout: 10_000 });

		// Type a look command
		const input = page.locator('[data-testid="input-field"] input');
		await input.fill('look');
		await input.press('Enter');

		// Chat panel should update with player input echo and system response
		const chatPanel = page.locator('[data-testid="chat-panel"]');
		await expect(chatPanel).toContainText('> look', { timeout: 5_000 });
	});

	test('player can move to a location', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('[data-testid="status-bar"]')).toBeVisible({ timeout: 10_000 });

		const input = page.locator('[data-testid="input-field"] input');
		await input.fill('go to church');
		await input.press('Enter');

		// Should see travel narration or "not found" message in the chat
		const chatPanel = page.locator('[data-testid="chat-panel"]');
		await expect(chatPanel).toContainText(/church|faintest notion/i, { timeout: 5_000 });
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
		await expect(page.locator('[data-testid="status-bar"]')).toBeVisible({ timeout: 10_000 });

		// Wait for theme to apply
		await page.waitForTimeout(1000);
		await page.screenshot({ path: 'e2e-results/initial-load.png', fullPage: true });

		// After a command
		const input = page.locator('[data-testid="input-field"] input');
		await input.fill('/status');
		await input.press('Enter');
		await page.waitForTimeout(500);
		await page.screenshot({ path: 'e2e-results/after-status.png', fullPage: true });
	});
});
