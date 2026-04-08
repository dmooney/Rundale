/**
 * Core E2E tests: verify the full app renders with mocked Tauri IPC.
 */

import { test, expect, installTauriMock, emitEvent, applyTheme } from './fixtures';
import { SNAPSHOTS, PALETTES, NPCS, MAP_DATA } from './mock-data';

test.describe('App layout', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('renders the app shell with all major sections', async ({ page }) => {
		await expect(page.locator('.app-shell')).toBeVisible();
		await expect(page.getByText('Baile Átha Cliath')).toBeVisible();
	});

	test('status bar shows time label and weather', async ({ page }) => {
		// StatusBar derives time label from game_epoch_ms (hour 8 → "Morning")
		await expect(page.locator('.time-label')).toContainText('Morning');
		await expect(page.getByText('Clear')).toBeVisible();
		await expect(page.getByText('Spring')).toBeVisible();
	});

	test('chat panel shows initial location description', async ({ page }) => {
		await expect(
			page.getByText('The streets of Dublin bustle with life', { exact: false })
		).toBeVisible();
	});

	test('map panel renders SVG with location markers', async ({ page }) => {
		const svg = page.locator('svg');
		await expect(svg).toBeVisible();
		const circles = svg.locator('circle');
		await expect(circles).toHaveCount(MAP_DATA.locations.length);
	});

	test('NPC chip row shows NPCs at current location', async ({ page }) => {
		// NPCs are now rendered as a clickable chip row above the input field,
		// not in the sidebar.
		for (const npc of NPCS) {
			await expect(
				page.locator('.npc-chip', { hasText: npc.name })
			).toBeVisible();
		}
	});

	test('input field is visible and enabled', async ({ page }) => {
		const input = page.locator('.input-field');
		await expect(input).toBeVisible();
		await expect(input).toBeEnabled();
	});

	test('sidebar shows name pronunciation hints from world snapshot', async ({ page }) => {
		// Verify pronunciation hints appear in the Focail panel
		await expect(page.getByText('[EE-fa]')).toBeVisible();
		await expect(page.getByText('— beauty, radiance')).toBeVisible();
		await expect(page.getByText('[BAHL-ya AH-ha KLEE-ah]')).toBeVisible();
	});
});

test.describe('Theme application', () => {
	test('default CSS variables are applied on load', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// The default palette from theme.ts is applied before any event
		const bgColor = await page.evaluate(() =>
			getComputedStyle(document.documentElement).getPropertyValue('--color-bg').trim()
		);
		expect(bgColor).toBe('#1a1a2e');
	});

	test('theme updates when theme-update event is emitted', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// Emit morning theme
		await applyTheme(page, PALETTES.morning);
		await page.waitForTimeout(300);

		let bgColor = await page.evaluate(() =>
			getComputedStyle(document.documentElement).getPropertyValue('--color-bg').trim()
		);
		expect(bgColor).toBe(PALETTES.morning.bg);

		// Switch to night theme
		await applyTheme(page, PALETTES.night);
		await page.waitForTimeout(300);

		bgColor = await page.evaluate(() =>
			getComputedStyle(document.documentElement).getPropertyValue('--color-bg').trim()
		);
		expect(bgColor).toBe(PALETTES.night.bg);
	});
});

test.describe('Event handling', () => {
	test('text-log event adds entry to chat panel', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await emitEvent(page, 'text-log', {
			source: 'system',
			content: 'You arrive at the market square.'
		});
		await page.waitForTimeout(300);

		await expect(page.getByText('You arrive at the market square.')).toBeVisible();
	});

	test('world-update event refreshes status bar', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// Verify initial state
		await expect(page.locator('.time-label')).toContainText('Morning');

		// Emit world update to midday
		await emitEvent(page, 'world-update', SNAPSHOTS.midday);
		await page.waitForTimeout(500);

		await expect(page.locator('.time-label')).toContainText('Midday');
	});
});
