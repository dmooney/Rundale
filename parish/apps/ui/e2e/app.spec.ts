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
import type { Page } from '@playwright/test';
import { SNAPSHOTS, PALETTES, NPCS } from './mock-data';
import { DEFAULT_THEME_PALETTE } from '../src/lib/theme';

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';

async function waitForNotebook(page: Page): Promise<void> {
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
	await expect(page.locator(PIXI_CANVAS)).toBeVisible();
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);
}

async function activateNotebookControl(
	page: Page,
	name: string,
): Promise<void> {
	const control = page.getByRole('button', { name, exact: true });
	await expect(control).toHaveCount(1);
	await expect(control).toBeEnabled();
	await control.focus();
	await expect(control).toBeFocused();
	await page.keyboard.press('Enter');
}

async function openJournal(page: Page) {
	await activateNotebookControl(page, 'Open Journal notebook tab');
	const journal = page.getByRole('dialog', {
		name: 'Parish Journal',
		exact: true,
	});
	await expect(journal).toBeVisible();
	return journal;
}

async function openFocail(page: Page) {
	await activateNotebookControl(page, 'Open notebook tools');
	const tools = page.getByRole('dialog', {
		name: 'More from the Notebook',
		exact: true,
	});
	await expect(tools).toBeVisible();
	await tools.getByRole('button', { name: /^Focail/ }).click();
	const focail = page.getByRole('dialog', {
		name: 'Focail — Irish Words',
		exact: true,
	});
	await expect(focail).toBeVisible();
	return focail;
}

test.describe('App layout', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
	});

	test('renders a clean illustrated-notebook first viewport', async ({
		page,
	}) => {
		await expect(page.locator('.app-shell')).toBeVisible();
		await expect(page.getByTestId('illustrated-notebook-game')).toHaveAttribute(
			'aria-hidden',
			'false',
		);
		await expect(page.locator(PIXI_CANVAS)).toBeVisible();

		// Legacy dashboard surfaces are notebook overlays, not the default shell.
		await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);
		await expect(page.getByTestId('status-bar')).toHaveCount(0);
		await expect(page.getByTestId('chat-panel')).toHaveCount(0);
		await expect(page.getByTestId('sidebar')).toHaveCount(0);
		await expect(page.getByTestId('map-panel')).toHaveCount(0);
		await expect(page.getByTestId('input-field')).toHaveCount(0);
		await expect(page.locator('.input-wrapper')).toHaveCount(0);
		await expect(page.locator('.input-form')).toHaveCount(0);
	});

	test('time card opens the current clock and weather notes', async ({
		page,
	}) => {
		await activateNotebookControl(page, 'Open time and weather');
		const notes = page.getByRole('dialog', {
			name: 'Time & Weather',
			exact: true,
		});
		await expect(notes).toBeVisible();
		await expect(notes.getByText('08:00', { exact: true })).toBeVisible();
		await expect(notes.getByText('Clear', { exact: true })).toBeVisible();
		await expect(notes.getByText('Spring', { exact: true })).toBeVisible();
	});

	test('Journal shows the initial location description', async ({ page }) => {
		const journal = await openJournal(page);
		await expect(
			journal.getByText('The streets of Dublin bustle with life', {
				exact: false,
			}),
		).toBeVisible();
	});

	test('Map card opens the parish MapLibre canvas', async ({ page }) => {
		await activateNotebookControl(page, 'Open parish map');
		const map = page.getByRole('dialog', {
			name: 'Parish Map',
			exact: true,
		});
		await expect(map).toBeVisible();
		const canvas = map
			.getByTestId('full-map')
			.locator('canvas.maplibregl-canvas');
		await expect(canvas).toBeVisible();
	});

	test('scene and Nearby controls expose every present NPC', async ({
		page,
	}) => {
		for (const npc of NPCS) {
			await expect(
				page.getByRole('button', {
					name: `Select marker for ${npc.name}`,
					exact: true,
				}),
			).toHaveCount(1);
			await expect(
				page.getByRole('button', {
					name: `Select nearby person ${npc.name}`,
					exact: true,
				}),
			).toHaveCount(1);
		}
	});

	test('People tab lists NPCs at the current location', async ({ page }) => {
		await activateNotebookControl(page, 'Open People notebook tab');
		const people = page.getByRole('dialog', {
			name: 'People of the Parish',
			exact: true,
		});
		await expect(people).toBeVisible();
		for (const npc of NPCS) {
			await expect(people.getByText(npc.name, { exact: true })).toBeVisible();
			await expect(
				people.getByText(new RegExp(`^${npc.occupation}\\s*[·•]`)),
			).toBeVisible();
		}
	});

	test('hidden native intent control accepts keyboard input at idle', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent', { exact: true });
		await expect(input).toHaveCount(1);
		await expect(input).toBeEnabled();
		await expect(input).toBeEditable();
		await expect(input).toHaveAttribute('type', 'text');
		await expect(input).not.toHaveAttribute('aria-disabled');
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await input.focus();
		await expect(input).toBeFocused();
		await page.keyboard.insertText('ask what happened');
		await expect(input).toHaveValue('ask what happened');
	});

	test('Focail sheet shows pronunciation hints from world snapshot', async ({
		page,
	}) => {
		const focail = await openFocail(page);
		await expect(focail.getByText('[EE-fa]')).toBeVisible();
		await expect(focail.getByText('— beauty, radiance')).toBeVisible();
		await expect(focail.getByText('[BAHL-ya AH-ha KLEE-ah]')).toBeVisible();
	});

	test('More routes each utility into one contained notebook sheet', async ({
		page,
	}) => {
		const routes = [
			{
				button: /^Focail/,
				surface: 'focail',
				dialog: 'Focail — Irish Words',
				close: 'Close Focail — Irish Words',
				childOwnsDialog: false,
			},
			{
				button: /^Save \/ Load/,
				surface: 'save',
				dialog: 'The Parish Ledger',
				close: 'Close',
				childOwnsDialog: true,
			},
			{
				button: /^Debug/,
				surface: 'debug',
				dialog: 'Parish Records',
				close: 'Close Parish Records',
				childOwnsDialog: false,
			},
			{
				button: /^Mod/,
				surface: 'mod',
				dialog: 'Select mod',
				close: 'Close',
				childOwnsDialog: true,
			},
			{
				button: /^Bug Report/,
				surface: 'bug',
				dialog: 'Report a bug',
				close: 'Close',
				childOwnsDialog: true,
			},
			{
				button: /^Shortcuts/,
				surface: 'shortcuts',
				dialog: 'Keyboard shortcuts',
				close: 'Close shortcuts',
				childOwnsDialog: true,
			},
		] as const;

		for (const route of routes) {
			await activateNotebookControl(page, 'Open notebook tools');
			const tools = page.getByRole('dialog', {
				name: 'More from the Notebook',
				exact: true,
			});
			await expect(tools).toBeVisible();
			await tools.getByRole('button', { name: route.button }).click();

			const host = page.getByTestId(`notebook-overlay-${route.surface}`);
			const dialog = page.getByRole('dialog', {
				name: route.dialog,
				exact: true,
			});
			await expect(host).toBeVisible({ timeout: 10_000 });
			await expect(dialog).toBeVisible({ timeout: 10_000 });
			await expect(dialog).toHaveAttribute('aria-modal', 'true');
			await expect(page.getByRole('dialog')).toHaveCount(1);
			if (route.childOwnsDialog) {
				await expect(host).not.toHaveAttribute('role');
			} else {
				await expect(host).toHaveAttribute('role', 'dialog');
			}

			await dialog
				.getByRole('button', { name: route.close, exact: true })
				.click();
			await expect(dialog).toHaveCount(0);
			await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(
				0,
			);
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
	test('text-log event adds entry to chat panel', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);

		await emitEvent(page, 'text-log', {
			source: 'system',
			content: 'You arrive at the market square.',
		});

		const journal = await openJournal(page);
		await expect(
			journal.getByText('You arrive at the market square.'),
		).toBeVisible();
	});

	test('world-update event refreshes time and weather notes', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await waitForNotebook(page);
		const summary = page.locator('.notebook-screenreader-summary');
		await expect(summary).toContainText('Morning');
		await activateNotebookControl(page, 'Open time and weather');
		const notes = page.getByRole('dialog', {
			name: 'Time & Weather',
			exact: true,
		});

		// Verify initial state
		await expect(notes.getByText('08:00', { exact: true })).toBeVisible();
		await expect(notes.getByText('Clear', { exact: true })).toBeVisible();

		// Emit world update to midday
		await emitEvent(page, 'world-update', SNAPSHOTS.midday);

		await expect(notes.getByText('12:00', { exact: true })).toBeVisible();
		await expect(notes.getByText('Overcast', { exact: true })).toBeVisible();
		await expect(summary).toContainText('Midday');
	});
});
