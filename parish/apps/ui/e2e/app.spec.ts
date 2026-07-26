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

test.describe('App layout', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('renders the app shell with all major sections', async ({ page }) => {
		await expect(page.locator('.app-shell')).toBeVisible();
		await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
		await expect(
			page.getByTestId('illustrated-notebook-pixi-host').locator('canvas'),
		).toBeVisible();
		await expect(
			page.getByTestId('illustrated-notebook-pixi-host'),
		).toHaveAttribute('data-scene-location-id', '404');
		await expect(page.getByTestId('notebook-status-summary')).toContainText(
			'Location: Baile Átha Cliath',
		);
		await expect(page.getByLabel('Live chronicle')).toContainText(
			'The streets of Dublin bustle with life',
		);
	});

	test('time drawer shows the canonical clock, weather, and season', async ({
		page,
	}) => {
		const timeControl = page.getByRole('button', {
			name: 'Open time details',
		});
		await timeControl.focus();
		await page.keyboard.press('Enter');

		const drawer = page.getByLabel('time drawer');
		await expect(drawer).toBeVisible();
		await expect(drawer).toContainText('08:00');
		await expect(drawer).toContainText('Morning');
		await expect(drawer).toContainText('Weather: Clear');
		await expect(drawer).toContainText('Season: Spring');
	});

	test('live chronicle shows initial location description', async ({
		page,
	}) => {
		await expect(page.getByLabel('Live chronicle')).toContainText(
			'The streets of Dublin bustle with life',
		);
	});

	test('notebook map control opens a MapLibre canvas', async ({ page }) => {
		const mapControl = page.getByRole('button', {
			name: 'Open parish map',
		});
		await mapControl.focus();
		await page.keyboard.press('Enter');

		const fullMap = page.getByTestId('full-map');
		await expect(fullMap).toBeVisible();
		const canvas = fullMap.locator('canvas.maplibregl-canvas');
		await expect(canvas).toBeVisible();
	});

	test('nearby-person controls show NPCs at the current location', async ({
		page,
	}) => {
		for (const npc of NPCS) {
			await expect(
				page.getByRole('button', {
					name: `Select nearby person ${npc.name}`,
				}),
			).toHaveCount(1);
		}
	});

	test('input field is visible and enabled', async ({ page }) => {
		const input = page.getByLabel('Player intent');
		await expect(input).toBeVisible();
		await expect(input).toBeEditable();
		await expect(input).toHaveAttribute('aria-busy', 'false');
	});

	test('people notebook shows world-derived NPC context', async ({ page }) => {
		const peopleControl = page.getByRole('button', {
			name: 'Open People notebook tab',
		});
		await peopleControl.focus();
		await page.keyboard.press('Enter');

		const drawer = page.getByLabel('people drawer');
		await expect(drawer).toBeVisible();
		for (const npc of NPCS) {
			await expect(drawer.getByText(npc.name)).toBeVisible();
			await expect(
				drawer.getByText(npc.occupation ?? 'occupation not recorded'),
			).toBeVisible();
		}
	});

	test('world pronunciation hints remain available in notebook context', async ({
		page,
	}) => {
		const notesControl = page.getByRole('button', {
			name: 'Open Notes notebook tab',
		});
		await notesControl.focus();
		await page.keyboard.press('Enter');
		const notes = page.getByLabel('notes drawer');
		await expect(notes.getByText('EE-fa')).toBeVisible();
		await expect(notes.getByText('beauty, radiance')).toBeVisible();
		await expect(notes.getByText('BAHL-ya AH-ha KLEE-ah')).toBeVisible();
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
	test('text-log event adds entry to the live chronicle', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await emitEvent(page, 'text-log', {
			source: 'system',
			content: 'You arrive at the market square.',
		});

		await expect(page.getByLabel('Live chronicle')).toContainText(
			'Parish: You arrive at the market square.',
		);
	});

	test('world-update event refreshes notebook time details', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await emitEvent(page, 'world-update', SNAPSHOTS.midday);

		const timeControl = page.getByRole('button', {
			name: 'Open time details',
		});
		await timeControl.focus();
		await page.keyboard.press('Enter');
		const drawer = page.getByLabel('time drawer');
		await expect(drawer).toContainText('12:00');
		await expect(drawer).toContainText('Midday');
		await expect(drawer).toContainText('Weather: Overcast');
	});
});
