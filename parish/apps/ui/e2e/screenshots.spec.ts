/**
 * Screenshot capture: generates docs/screenshots/gui-{time}.png
 * and creates visual regression baselines.
 *
 * Run: npx playwright test e2e/screenshots.spec.ts
 * Update baselines: npx playwright test e2e/screenshots.spec.ts --update-snapshots
 */

import {
	test,
	expect,
	installTauriMock,
	applyTheme,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';
import type { Page } from '@playwright/test';
import { PALETTES, SNAPSHOTS } from './mock-data';
import type { NpcInfo } from '../src/lib/types';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TIMES_OF_DAY = ['morning', 'midday', 'dusk', 'night'] as const;
// Path is relative to apps/ui/e2e/screenshots.spec.ts → parish/docs/screenshots/.
const SCREENSHOT_DIR = path.resolve(__dirname, '../../../docs/screenshots');
const SCREENSHOT_NPCS: NpcInfo[] = [
	{
		npc_id: 4,
		name: 'Roisin Connolly',
		real_name: 'Roisin Connolly',
		occupation: 'Shopkeeper',
		mood: 'wary',
		introduced: true,
		mood_emoji: '•',
	},
	{
		npc_id: 1,
		name: 'Padraig Darcy',
		real_name: 'Padraig Darcy',
		occupation: 'Publican',
		mood: 'content',
		introduced: true,
		mood_emoji: '•',
	},
	{
		npc_id: 2,
		name: 'Siobhan Murphy',
		real_name: 'Siobhan Murphy',
		occupation: 'Farmer',
		mood: 'determined',
		introduced: true,
		mood_emoji: '•',
	},
	{
		npc_id: 3,
		name: 'Fr. Declan Tierney',
		real_name: 'Fr. Declan Tierney',
		occupation: 'Parish Priest',
		mood: 'contemplative',
		introduced: true,
		mood_emoji: '•',
	},
];

async function expectRenderedNotebookScreenshot(
	page: Page,
	png: Buffer,
): Promise<void> {
	const stats = await page.evaluate(async (base64) => {
		const image = new Image();
		image.src = `data:image/png;base64,${base64}`;
		await image.decode();
		const sample = document.createElement('canvas');
		sample.width = 32;
		sample.height = 20;
		const context = sample.getContext('2d', { willReadFrequently: true });
		if (!context) return { nonBlackRatio: 0, colourBuckets: 0 };
		context.drawImage(image, 0, 0, sample.width, sample.height);
		const pixels = context.getImageData(0, 0, sample.width, sample.height).data;
		let nonBlack = 0;
		const colourBuckets = new Set<number>();
		for (let index = 0; index < pixels.length; index += 4) {
			const red = pixels[index];
			const green = pixels[index + 1];
			const blue = pixels[index + 2];
			const alpha = pixels[index + 3];
			if (alpha > 0 && red + green + blue > 60) nonBlack += 1;
			colourBuckets.add((red >> 4) * 256 + (green >> 4) * 16 + (blue >> 4));
		}
		return {
			nonBlackRatio: nonBlack / (pixels.length / 4),
			colourBuckets: colourBuckets.size,
		};
	}, png.toString('base64'));

	expect(
		stats.nonBlackRatio,
		'generated notebook screenshot must not contain cleared black WebGL regions',
	).toBeGreaterThanOrEqual(0.7);
	expect(
		stats.colourBuckets,
		'generated notebook screenshot must contain a varied rendered scene',
	).toBeGreaterThanOrEqual(20);
}

/**
 * Shared page setup for both the screenshot-generation and visual-regression
 * suites (TD-041): installs the Tauri mock for `time`, navigates, and waits for
 * the illustrated Pixi scene to finish loading. The default capture deliberately
 * leaves every notebook overlay closed so it records the clean first viewport.
 */
async function setupScreenshotPage(
	page: Page,
	time: (typeof TIMES_OF_DAY)[number],
): Promise<void> {
	await installTauriMock(page, time, {
		npcs: SCREENSHOT_NPCS,
		snapshot: {
			...SNAPSHOTS[time],
			location_name: 'Kilteevan Village',
			location_description:
				'The crossroads at Kilteevan are damp after rain, with cottages, low walls, and neighbours moving through the morning.',
			name_hints: [],
		},
	});
	await page.goto('/');
	await page.waitForLoadState('networkidle');

	await expect(
		page.locator('[data-testid="illustrated-notebook-pixi-host"] canvas'),
	).toBeVisible();
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);
	await expect(
		page.getByRole('button', {
			name: 'Select nearby person Roisin Connolly',
			exact: true,
		}),
	).toHaveCount(1);
	await applyTheme(page, PALETTES[time]);

	const summary = page.getByRole('status', { name: 'Parish status' });
	await expect(summary).toContainText(SNAPSHOTS[time].time_label);
	const timeControl = page.getByRole('button', {
		name: 'Open time and weather',
		exact: true,
	});
	await expect(timeControl).toHaveCount(1);
	await expect(timeControl).toBeEnabled();
	await timeControl.focus();
	await expect(timeControl).toBeFocused();
	await page.keyboard.press('Enter');
	const notes = page.getByRole('dialog', {
		name: 'Time & Weather',
		exact: true,
	});
	await expect(notes).toBeVisible();
	await expect(
		notes.getByText(
			`${String(SNAPSHOTS[time].hour).padStart(2, '0')}:${String(
				SNAPSHOTS[time].minute,
			).padStart(2, '0')}`,
			{ exact: true },
		),
	).toBeVisible();
	await expect(
		notes.getByText(SNAPSHOTS[time].weather, { exact: true }),
	).toBeVisible();
	await expect(
		notes.getByText(SNAPSHOTS[time].season, { exact: true }),
	).toBeVisible();
	await notes
		.getByRole('button', { name: 'Close Time & Weather', exact: true })
		.click();
	await expect(notes).toHaveCount(0);
	await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);
	await expect(page.getByTestId('chat-panel')).toHaveCount(0);

	await page.evaluate(async () => {
		await document.fonts.ready;
		await new Promise<void>((resolve) =>
			requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
		);
	});
	await expect(page.getByTestId('illustrated-notebook-game')).toHaveAttribute(
		'aria-hidden',
		'false',
	);
	await waitForTextureCompleteNotebookFrame(page);
}

test.describe('Screenshot generation', () => {
	for (const time of TIMES_OF_DAY) {
		test(`capture gui-${time}`, async ({ page }) => {
			await setupScreenshotPage(page, time);

			// Save to docs/screenshots/ for the project
			const png = await page.screenshot({
				path: path.join(SCREENSHOT_DIR, `gui-${time}.png`),
				fullPage: false,
			});
			await expectRenderedNotebookScreenshot(page, png);
		});
	}
});

test.describe('Visual regression baselines', () => {
	// Baselines are environment-specific (fonts, browser pixel rendering).
	// Skip in CI; run manually with `--update-snapshots` to refresh locally.
	test.skip(
		!!process.env.CI,
		'visual-regression baselines are environment-specific',
	);

	for (const time of TIMES_OF_DAY) {
		test(`visual-regression-${time}`, async ({ page }) => {
			await setupScreenshotPage(page, time);

			// Playwright visual comparison (stores baselines in snapshotDir)
			await expect(page).toHaveScreenshot(`gui-${time}.png`, {
				maxDiffPixelRatio: 0.02,
			});
		});
	}
});
