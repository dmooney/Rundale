import { test, expect } from '@playwright/test';
import { installTileRouteMock } from './fixtures';

test.describe('Parish Web UI', () => {
	test.beforeEach(async ({ page }) => {
		await installTileRouteMock(page);
	});

	test('page loads with game state', async ({ page }) => {
		await page.goto('/');

		// Status bar should show a time-of-day label
		const statusBar = page.locator('[data-testid="status-bar"]');
		await expect(statusBar).toBeVisible({ timeout: 10_000 });
		await expect(statusBar).toContainText(
			/Morning|Midday|Afternoon|Dusk|Night|Dawn/,
		);

		// Chat panel should have the initial location description
		const chatPanel = page.locator('[data-testid="chat-panel"]');
		await expect(chatPanel).toBeVisible();
		await expect(chatPanel).not.toBeEmpty();

		// Input field should be present
		const inputField = page.locator('[data-testid="input-field"]');
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
		await expect(page.locator('[data-testid="status-bar"]')).toBeVisible({
			timeout: 10_000,
		});

		// Type a look command
		const input = page.locator('[data-testid="input-field"]');
		await input.fill('look');
		await input.press('Enter');

		// Since #1351, a bare `look` is routed as a game action, NOT echoed as
		// a player speech bubble. The chat panel should receive a new system
		// message (the location description) rather than a `> look` speech line.
		// The real web server always produces exactly 3 system entries after a look:
		//   1. splash_text — prepended by getUiConfig() on page load; the server
		//      always returns a non-empty splash (game title + copyright line).
		//   2. initial location description — from getWorldSnapshot().location_description
		//      on page load.
		//   3. look result — text-log {source:"system"} emitted by handle_look.
		// Timeout is 30 s, not 5 s, because the chat panel only updates after
		// the backend's first round-trip — which on a cold-start CI runner
		// can exceed 5 s when the inference worker hasn't warmed up (#1086).
		const chatPanel = page.locator('[data-testid="chat-panel"]');
		const systemEntries = chatPanel.locator('.entry.system');
		await expect(systemEntries).toHaveCount(3, { timeout: 30_000 });
	});

	test('player can move to a location', async ({ page }) => {
		await page.goto('/');
		await expect(page.locator('[data-testid="status-bar"]')).toBeVisible({
			timeout: 10_000,
		});

		const input = page.locator('[data-testid="input-field"]');
		await input.fill('go to church');
		await input.press('Enter');

		// Should see travel narration or "not found" message in the chat.
		// Timeout is 30 s for the same cold-start reason as the sibling
		// 'player can type a command' test above (#1086).
		const chatPanel = page.locator('[data-testid="chat-panel"]');
		await expect(chatPanel).toContainText(/church|faintest notion/i, {
			timeout: 30_000,
		});
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
		await expect(page.locator('[data-testid="status-bar"]')).toBeVisible({
			timeout: 10_000,
		});

		// Wait for the app shell to be fully rendered before taking the screenshot.
		await expect(page.locator('.app-shell')).toBeVisible();
		await page.screenshot({
			path: 'e2e-results/initial-load.png',
			fullPage: true,
		});

		// After a command
		const input = page.locator('[data-testid="input-field"]');
		await input.fill('/status');
		await input.press('Enter');
		await expect(page.locator('[data-testid="chat-panel"]')).toContainText(
			'Location:',
			{ timeout: 5_000 },
		);
		await page.screenshot({
			path: 'e2e-results/after-status.png',
			fullPage: true,
		});
	});
});
