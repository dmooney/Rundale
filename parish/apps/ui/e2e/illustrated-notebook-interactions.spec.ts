/**
 * Visual and interaction proof for the fresh illustrated-notebook rebuild
 * (#1630). Proof images are written to the repo-root `.proofs/1630/` bundle.
 *
 * The Pixi controls expose semantic companion buttons, so this spec exercises
 * those controls by accessible name instead of depending on scene coordinates.
 */

import type { Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { expect, installTauriMock, test } from './fixtures';
import type { MapData, NpcInfo, WorldSnapshot } from '../src/lib/types';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/1630');

type CanvasBounds = NonNullable<
	Awaited<ReturnType<ReturnType<Page['locator']>['boundingBox']>>
>;

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';

const RUNDALE_PROOF_WORLD: WorldSnapshot = {
	location_name: 'The Crossroads',
	location_description:
		'A quiet crossroads where four narrow roads meet beneath a clearing spring sky.',
	time_label: 'Afternoon',
	hour: 15,
	minute: 40,
	weather: 'clearing',
	season: 'Spring',
	festival: null,
	paused: false,
	inference_paused: false,
	game_epoch_ms: Date.UTC(1820, 3, 1, 15, 40),
	speed_factor: 0,
	name_hints: [],
	day_of_week: 'Monday',
};

const RUNDALE_PROOF_MAP: MapData = {
	locations: [
		{
			id: '1',
			name: 'The Crossroads',
			lat: 53.63621,
			lon: -8.11531,
			adjacent: false,
			hops: 0,
		},
		{
			id: '3',
			name: "St. Brigid's Church",
			lat: 53.63794,
			lon: -8.10399,
			adjacent: true,
			hops: 1,
		},
		{
			id: '13',
			name: "Connolly's Shop",
			lat: 53.63617,
			lon: -8.11502,
			adjacent: true,
			hops: 1,
		},
		{
			id: '15',
			name: 'Kilteevan Village',
			lat: 53.63254,
			lon: -8.10217,
			adjacent: true,
			hops: 1,
		},
	],
	edges: [
		['1', '3'],
		['1', '13'],
		['1', '15'],
	],
	player_location: '1',
	player_lat: 53.63621,
	player_lon: -8.11531,
};

const RUNDALE_PROOF_NPCS: NpcInfo[] = [
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

async function settlePaint(page: Page): Promise<void> {
	await page.evaluate(
		() =>
			new Promise<void>((resolve) =>
				requestAnimationFrame(() => requestAnimationFrame(() => resolve())),
			),
	);
}

async function canvasBounds(page: Page): Promise<CanvasBounds> {
	const bounds = await page.locator(PIXI_CANVAS).boundingBox();
	if (!bounds)
		throw new Error('Illustrated parish canvas has no layout bounds');
	return bounds;
}

async function expectCanvasBounds(
	page: Page,
	expected: CanvasBounds,
): Promise<void> {
	const actual = await canvasBounds(page);
	for (const key of ['x', 'y', 'width', 'height'] as const) {
		expect(
			actual[key],
			`canvas ${key} changed while routing an overlay`,
		).toBeCloseTo(expected[key], 2);
	}
}

async function activateNotebookControl(page: Page, name: string) {
	const control = page.getByRole('button', { name, exact: true });
	await expect(control).toHaveCount(1);
	await expect(control).toBeEnabled();
	await control.focus();
	await expect(control).toBeFocused();
	await page.keyboard.press('Enter');
	return control;
}

async function clickNotebookCanvasTarget(page: Page, name: string) {
	const control = page.getByRole('button', { name, exact: true });
	await expect(control).toHaveCount(1);
	const bounds = await control.boundingBox();
	if (!bounds) throw new Error(`Notebook control "${name}" has no bounds`);
	await page.mouse.click(
		bounds.x + bounds.width / 2,
		bounds.y + bounds.height / 2,
	);
}

async function expectCleanFirstViewport(page: Page): Promise<void> {
	const game = page.getByTestId('illustrated-notebook-game');
	await expect(game).toBeVisible();
	await expect(game).toHaveAttribute('aria-hidden', 'false');
	await expect(game).not.toHaveAttribute('inert', '');
	await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);

	for (const selector of [
		'.input-wrapper',
		'.input-form',
		'[data-testid="chat-panel"]',
		'[data-testid="full-map"]',
		'[data-testid="save-picker"]',
		'[data-testid="debug-panel"]',
		'[data-testid="bug-report-modal"]',
		'[data-testid="shortcuts-overlay"]',
		'[role="dialog"][aria-label="Select mod"]',
	]) {
		await expect(page.locator(selector)).toHaveCount(0);
	}
}

async function setupNotebookPage(page: Page): Promise<void> {
	await installTauriMock(page, 'morning', {
		snapshot: RUNDALE_PROOF_WORLD,
		mapData: RUNDALE_PROOF_MAP,
		npcs: RUNDALE_PROOF_NPCS,
	});
	await page.goto('/');
	await page.waitForLoadState('networkidle');

	await expect(page.locator(PIXI_CANVAS)).toBeVisible();
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await page
		.getByLabel('Player intent', { exact: true })
		.fill('ask Roisin what she saw.');
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);
	await expectCleanFirstViewport(page);
	await settlePaint(page);
}

test.describe('illustrated notebook overlays (#1630)', () => {
	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
	});

	test('browser decodes every documented v2 raster asset', async ({ page }) => {
		const response = await page.request.get(
			'/rundale/illustrated-notebook-v2/ui-assets.json',
		);
		expect(response.ok()).toBe(true);
		const manifest = (await response.json()) as {
			runtime_base: string;
			assets: Array<{ file: string; width: number; height: number }>;
		};
		expect(manifest.assets.length).toBeGreaterThan(3);
		for (const asset of manifest.assets) {
			expect(asset.file).toMatch(/\.png$/);
			expect(asset.file).not.toMatch(/\.svg$/);
		}

		await installTauriMock(page, 'morning');
		await page.goto('/');
		const decoded = await page.evaluate(
			async ({ base, assets }) => {
				return Promise.all(
					assets.map(async (asset) => {
						const image = new Image();
						image.src = `${base}${asset.file}`;
						await image.decode();
						return {
							file: asset.file,
							width: image.naturalWidth,
							height: image.naturalHeight,
						};
					}),
				);
			},
			{ base: manifest.runtime_base, assets: manifest.assets },
		);
		expect(decoded).toEqual(
			manifest.assets.map(({ file, width, height }) => ({
				file,
				width,
				height,
			})),
		);
	});

	test('desktop keeps the first viewport clean and routes notebook tools without resizing Pixi', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);
		const initialBounds = await canvasBounds(page);

		for (const name of [
			'Open Journal notebook tab',
			'Open People notebook tab',
			'Open Places notebook tab',
			'Open parish map',
			'Open notebook tools',
		]) {
			await expect(page.getByRole('button', { name, exact: true })).toHaveCount(
				1,
			);
		}

		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop-first-viewport.png'),
			fullPage: false,
		});

		// Use the canvas itself (not the semantic companion button) to prove that
		// a translated and scaled raster Sprite owns the correct local hit area.
		await clickNotebookCanvasTarget(page, 'Open parish map');
		const mapOverlay = page.getByRole('dialog', {
			name: 'Parish Map',
			exact: true,
		});
		await expect(mapOverlay).toBeVisible();
		await page
			.getByRole('button', { name: 'Close Parish Map', exact: true })
			.click();
		await expectCleanFirstViewport(page);
		await expectCanvasBounds(page, initialBounds);

		const toolsInvoker = await activateNotebookControl(
			page,
			'Open notebook tools',
		);
		const toolsOverlay = page.getByRole('dialog', {
			name: 'More from the Notebook',
			exact: true,
		});
		await expect(toolsOverlay).toBeVisible();
		await expect(toolsOverlay).toHaveAttribute('data-surface', 'utility');
		await expect(page.getByTestId('illustrated-notebook-game')).toHaveAttribute(
			'aria-hidden',
			'true',
		);
		for (const name of [
			'Focail',
			'Save / Load',
			'Debug',
			'Mod',
			'Bug Report',
			'Shortcuts',
		]) {
			await expect(
				toolsOverlay.getByRole('button', { name: new RegExp(`^${name}`) }),
			).toBeVisible();
		}
		await expectCanvasBounds(page, initialBounds);

		await page
			.getByRole('button', {
				name: 'Close More from the Notebook',
				exact: true,
			})
			.click();
		await expectCleanFirstViewport(page);
		await expect(toolsInvoker).toBeFocused();
		await expectCanvasBounds(page, initialBounds);

		const journalInvoker = await activateNotebookControl(
			page,
			'Open Journal notebook tab',
		);
		const journalOverlay = page.getByRole('dialog', {
			name: 'Parish Journal',
			exact: true,
		});
		await expect(journalOverlay).toBeVisible();
		await expect(journalOverlay).toHaveAttribute('data-surface', 'journal');
		await expect(journalOverlay.getByTestId('chat-panel')).toBeVisible();
		await expectCanvasBounds(page, initialBounds);
		await page.keyboard.press('F10');
		await expect(
			page.getByRole('dialog', { name: 'Demo mode configuration' }),
		).toHaveCount(0);

		await settlePaint(page);
		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop-journal-overlay.png'),
			fullPage: false,
		});

		await page
			.getByRole('button', {
				name: 'Close Parish Journal',
				exact: true,
			})
			.click();
		await expectCleanFirstViewport(page);
		await expect(journalInvoker).toBeFocused();
		await expectCanvasBounds(page, initialBounds);
	});

	test('mobile keeps notebook controls usable and restores the vertical canvas after a drawer closes', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);
		const initialBounds = await canvasBounds(page);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile-first-viewport.png'),
			fullPage: false,
		});

		await activateNotebookControl(page, 'Ask action');
		await expect(
			page.getByRole('textbox', { name: 'Player intent', exact: true }),
		).toHaveValue(/^ask /i);

		const invoker = await activateNotebookControl(
			page,
			'Open People notebook tab',
		);
		const peopleOverlay = page.getByRole('dialog', {
			name: 'People of the Parish',
			exact: true,
		});
		await expect(peopleOverlay).toBeVisible();
		await expect(peopleOverlay).toHaveAttribute('data-surface', 'people');
		await expect(
			peopleOverlay.locator('.people-list button').first(),
		).toBeVisible();
		await expectCanvasBounds(page, initialBounds);

		await settlePaint(page);
		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile-people-overlay.png'),
			fullPage: false,
		});

		await page
			.getByRole('button', {
				name: 'Close People of the Parish',
				exact: true,
			})
			.click();
		await expectCleanFirstViewport(page);
		await expect(invoker).toBeFocused();
		await expectCanvasBounds(page, initialBounds);
	});
});
