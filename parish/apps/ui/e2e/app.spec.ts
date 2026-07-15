/**
 * Core E2E tests: verify the full app renders with mocked Tauri IPC.
 */

import {
	test,
	expect,
	installTauriMock,
	emitEvent,
	applyTheme,
} from './fixtures';
import { SNAPSHOTS, PALETTES, NPCS } from './mock-data';
import { DEFAULT_THEME_PALETTE } from '../src/lib/theme';
import type { Page } from '@playwright/test';

async function waitForNotebook(page: Page): Promise<void> {
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
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

test.describe('App layout', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
	});

	test('renders the illustrated notebook shell without retired dashboard chrome', async ({
		page,
	}) => {
		await expect(page.locator('.app-shell')).toBeVisible();
		await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
		await expect(
			page.getByTestId('illustrated-notebook-pixi-host').locator('canvas'),
		).toBeVisible();
		await expect(page.getByLabel('Player intent')).toHaveCount(1);

		for (const selector of [
			'[data-testid="status-bar"]',
			'[data-testid="chat-panel"]',
			'[data-testid="map-panel"]',
			'[data-testid="sidebar"]',
			'[data-testid="input-field"]',
			'.input-wrapper',
			'.input-form',
		]) {
			await expect(page.locator(selector)).toHaveCount(0);
		}
	});

	test('time drawer shows the current clock, weather, and season', async ({
		page,
	}) => {
		await activateNotebookControl(page, 'Open time details');
		const drawer = page.getByLabel('time drawer');
		await expect(drawer).toBeVisible();
		await expect(drawer).toContainText(/08:00\s*Morning/);
		await expect(drawer).toContainText('Weather: Clear');
		await expect(drawer).toContainText('Season: Spring');
	});

	test('journal drawer shows the initial location description', async ({
		page,
	}) => {
		await activateNotebookControl(page, 'Open Journal notebook tab');
		await expect(page.getByLabel('journal drawer')).toContainText(
			'The streets of Dublin bustle with life',
		);
	});

	test('notebook map control opens the full MapLibre map', async ({ page }) => {
		await activateNotebookControl(page, 'Open parish map');
		const map = page.getByTestId('full-map');
		await expect(map).toBeVisible();
		await expect(map.locator('canvas.maplibregl-canvas')).toBeVisible();
	});

	test('notebook markers and nearby controls expose every present NPC', async ({
		page,
	}) => {
		for (const npc of NPCS) {
			await expect(
				page.getByRole('button', { name: `Select marker for ${npc.name}` }),
			).toHaveCount(1);
			await expect(
				page.getByRole('button', {
					name: `Select nearby person ${npc.name}`,
				}),
			).toHaveCount(1);
		}
	});

	test('hidden native intent control accepts keyboard input at idle', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent');
		await expect(input).toHaveAttribute('aria-disabled', 'false');
		await input.focus();
		await expect(input).toBeFocused();
		await page.keyboard.type('ask what happened');
		await expect(input).toHaveValue('ask what happened');
	});

	test('people drawer replaces the persistent sidebar for nearby details', async ({
		page,
	}) => {
		await activateNotebookControl(page, 'Open People notebook tab');
		const drawer = page.getByLabel('people drawer');
		await expect(drawer).toBeVisible();
		for (const npc of NPCS) {
			await expect(drawer).toContainText(npc.name);
			await expect(drawer).toContainText(npc.occupation);
		}
	});
});

test.describe('Theme application', () => {
	test('default CSS variables are applied on load', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// The configured fixed palette is applied on load.
		const bgColor = await page.evaluate(() =>
			getComputedStyle(document.documentElement)
				.getPropertyValue('--color-bg')
				.trim(),
		);
		expect(bgColor).toBe(DEFAULT_THEME_PALETTE.bg);
	});

	test('theme updates when theme-update event is emitted', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// Emit morning theme
		await applyTheme(page, PALETTES.morning);
		await expect
			.poll(() =>
				page.evaluate(() =>
					getComputedStyle(document.documentElement)
						.getPropertyValue('--color-bg')
						.trim(),
				),
			)
			.toBe(PALETTES.morning.bg);

		// Switch to night theme
		await applyTheme(page, PALETTES.night);
		await expect
			.poll(() =>
				page.evaluate(() =>
					getComputedStyle(document.documentElement)
						.getPropertyValue('--color-bg')
						.trim(),
				),
			)
			.toBe(PALETTES.night.bg);
	});
});

test.describe('Event handling', () => {
	test('text-log event adds an entry to the journal drawer', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);

		await emitEvent(page, 'text-log', {
			source: 'system',
			content: 'You arrive at the market square.',
		});

		await activateNotebookControl(page, 'Open Journal notebook tab');
		await expect(page.getByLabel('journal drawer')).toContainText(
			'You arrive at the market square.',
		);
	});

	test('world-update event refreshes notebook time details', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);

		await activateNotebookControl(page, 'Open time details');
		const drawer = page.getByLabel('time drawer');
		await expect(drawer).toContainText(/08:00\s*Morning/);

		await emitEvent(page, 'world-update', SNAPSHOTS.midday);

		await expect(drawer).toContainText(/12:00\s*Midday/);
		await expect(drawer).toContainText('Weather: Overcast');
	});
});
