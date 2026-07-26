/**
 * Visual and interaction proof for the illustrated-notebook interaction model
 * (#1755). Proof images are written to the repo-root `.proofs/1755/` bundle.
 *
 * The Pixi controls expose semantic companion buttons, so this spec exercises
 * those controls by accessible name instead of depending on scene coordinates.
 */

import type { Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';
import {
	emitEvent,
	expect,
	installTauriMock,
	test,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';
import type { MapData, NpcInfo, WorldSnapshot } from '../src/lib/types';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/1755');
const COMMAND_PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/1626');
const COMMAND_VISUAL_PROOF_TIMEOUT_MS = 120_000;

type CanvasBounds = NonNullable<
	Awaited<ReturnType<ReturnType<Page['locator']>['boundingBox']>>
>;

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';

const RUNDALE_PROOF_WORLD: WorldSnapshot = {
	location_id: 1,
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
	active_tasks: [],
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
	transport_label: 'on foot',
	transport_id: 'walking',
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

async function settleNotebookFrame(page: Page): Promise<void> {
	await settlePaint(page);
	await waitForTextureCompleteNotebookFrame(page);
}

async function installControlledSubmitFailure(page: Page): Promise<void> {
	await page.evaluate(() => {
		type Invoke = (
			command: string,
			args?: Record<string, unknown>,
		) => Promise<unknown>;
		const globals = window as unknown as Record<string, unknown>;
		const internals = globals.__TAURI_INTERNALS__ as { invoke: Invoke };
		const originalInvoke = internals.invoke.bind(internals);
		const control: { reject: (reason?: unknown) => void } = {
			reject: () => {},
		};
		globals.__TEST_REJECT_NOTEBOOK_SUBMIT__ = () =>
			control.reject(new Error('bridge unavailable'));
		internals.invoke = (command, args) => {
			if (command !== 'submit_input') return originalInvoke(command, args);
			return new Promise<unknown>((_resolve, reject) => {
				control.reject = reject;
			});
		};
	});
}

async function rejectControlledSubmit(page: Page): Promise<void> {
	await page.evaluate(() => {
		const reject = (
			window as unknown as Record<string, (() => void) | undefined>
		).__TEST_REJECT_NOTEBOOK_SUBMIT__;
		if (!reject)
			throw new Error('controlled submit rejection was not installed');
		reject();
	});
}

async function proveCommandSurface(
	page: Page,
	viewport: 'desktop' | 'mobile',
): Promise<void> {
	const input = page.getByLabel('Player intent', { exact: true });
	const status = page.locator('#notebook-command-status');
	const askStamp = page.getByRole('button', {
		name: 'Ask action',
		exact: true,
	});
	const stableCanvas = await canvasBounds(page);

	await input.fill('look around');
	await input.press('Enter');
	await expect(input).toHaveValue('');
	await input.fill('talk to Roisin');
	await input.press('Enter');
	await expect(input).toHaveValue('');
	await input.fill('a draft worth keeping');
	await input.press('ArrowUp');
	await expect(input).toHaveValue('talk to Roisin');
	await input.press('ArrowUp');
	await expect(input).toHaveValue('look around');
	await input.press('ArrowDown');
	await expect(input).toHaveValue('talk to Roisin');
	await input.press('ArrowDown');
	await expect(input).toHaveValue('a draft worth keeping');
	await settleNotebookFrame(page);
	await page.screenshot({
		path: path.join(COMMAND_PROOF_DIR, `${viewport}-command-history.png`),
		fullPage: false,
	});

	await emitEvent(page, 'loading', { active: true, phrase: 'Listening...' });
	await emitEvent(page, 'stream-token', {
		token: 'The whole reply appears when the player chooses to move on.',
		turn_id: 1629,
		source: 'Roisin Connolly',
	});
	await emitEvent(page, 'stream-turn-end', { turn_id: 1629 });
	await expect(input).toHaveAttribute('aria-busy', 'true');
	await input.press('ArrowUp');
	await expect(input).toHaveValue('a draft worth keeping');
	await expect(input).toHaveAttribute('aria-busy', 'false');

	await input.fill('');
	await emitEvent(page, 'loading', { active: true, phrase: 'Listening...' });
	await expect(input).toHaveAttribute('data-command-state', 'busy');
	await expect(input).not.toHaveAttribute('aria-disabled');
	await expect(input).toHaveAttribute('aria-busy', 'true');
	await expect(input).toBeEditable();
	await expect(status).toHaveAttribute('role', 'status');
	await expect(status).not.toHaveAttribute('aria-live');
	await expect(status).toContainText('Parish reply in progress');
	await expect(askStamp).toBeEnabled();
	await settleNotebookFrame(page);
	await expectCleanFirstViewport(page);
	await expectCanvasBounds(page, stableCanvas);
	await page.screenshot({
		path: path.join(COMMAND_PROOF_DIR, `${viewport}-command-busy.png`),
		fullPage: false,
	});

	await emitEvent(page, 'loading', { active: false });
	await installControlledSubmitFailure(page);
	await input.fill('ask Roisin what she saw');
	await input.press('Enter');
	await expect(input).toHaveAttribute('data-command-state', 'disabled');
	await expect(input).toHaveAttribute('aria-disabled', 'true');
	await expect(input).toHaveAttribute('aria-busy', 'true');
	await expect(input).toHaveAttribute('readonly', '');
	await expect(input).not.toBeEditable();
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(askStamp).toBeDisabled();
	await askStamp.dispatchEvent('click');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(status).toHaveAttribute('role', 'status');
	await expect(status).toContainText('Sending your line');
	await settleNotebookFrame(page);
	await expectCanvasBounds(page, stableCanvas);
	await page.screenshot({
		path: path.join(COMMAND_PROOF_DIR, `${viewport}-command-disabled.png`),
		fullPage: false,
	});

	await rejectControlledSubmit(page);
	await expect(input).toHaveAttribute('data-command-state', 'error');
	await expect(input).not.toHaveAttribute('aria-disabled');
	await expect(input).toHaveAttribute('aria-busy', 'false');
	await expect(input).toHaveAttribute('aria-invalid', 'true');
	await expect(input).toHaveValue('ask Roisin what she saw');
	await expect(status).toHaveAttribute('role', 'alert');
	await expect(status).not.toHaveAttribute('aria-live');
	await expect(status).toContainText(
		'Ink blotted — Could not send input: bridge unavailable',
	);
	await expect(askStamp).toBeEnabled();
	await settleNotebookFrame(page);
	await expectCleanFirstViewport(page);
	await expectCanvasBounds(page, stableCanvas);
	await page.screenshot({
		path: path.join(COMMAND_PROOF_DIR, `${viewport}-command-error.png`),
		fullPage: false,
	});
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

test.describe('illustrated notebook interaction model (#1755)', () => {
	test.describe.configure({ timeout: COMMAND_VISUAL_PROOF_TIMEOUT_MS });

	test.beforeAll(() => {
		fs.mkdirSync(PROOF_DIR, { recursive: true });
		fs.mkdirSync(COMMAND_PROOF_DIR, { recursive: true });
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

	test('desktop turns notebook sections in place and reserves sheets for transient tools', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);
		const initialBounds = await canvasBounds(page);

		for (const name of [
			'Open Notes notebook tab',
			'Open Journal notebook tab',
			'Open People notebook tab',
			'Open Places notebook tab',
			'Open Rumours notebook tab',
			'Open parish map',
			'Open notebook tools',
		]) {
			await expect(page.getByRole('button', { name, exact: true })).toHaveCount(
				1,
			);
		}

		const section = page.getByTestId('notebook-active-section');
		for (const [tab, title] of [
			['notes', 'Parish Notes'],
			['people', 'Roisin Connolly'],
			['places', 'Places in this Parish'],
			['rumours', 'Rumours'],
			['journal', 'Parish Journal'],
		] as const) {
			const control = await activateNotebookControl(
				page,
				`Open ${tab.charAt(0).toUpperCase()}${tab.slice(1)} notebook tab`,
			);
			await expect(control).toHaveAttribute('aria-pressed', 'true');
			await expect(section).toHaveAttribute('data-section', tab);
			await expect(section).toContainText(title);
			await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(
				0,
			);
			await expectCanvasBounds(page, initialBounds);
			await settleNotebookFrame(page);
			await page.screenshot({
				path: path.join(PROOF_DIR, `desktop-section-${tab}.png`),
				fullPage: false,
			});
		}

		await activateNotebookControl(page, 'Open Places notebook tab');
		await expect(section).toContainText("St. Brigid's Church");
		await expect(section).toContainText('Open the Map card below');
		await expect(page.getByLabel('Player intent', { exact: true })).toHaveValue(
			'ask Roisin what she saw.',
		);

		// Use the canvas itself (not the semantic companion button) to prove that
		// the geographically distinct Map card owns the correct local hit area.
		await clickNotebookCanvasTarget(page, 'Open parish map');
		const mapOverlay = page.getByRole('dialog', {
			name: 'Parish Map',
			exact: true,
		});
		await expect(mapOverlay).toBeVisible();
		await expect(mapOverlay).toHaveAttribute('data-surface', 'map');
		await expect(page.getByLabel('Map controls')).toContainText(
			'scroll or pinch to zoom',
		);
		await settlePaint(page);
		await page.screenshot({
			path: path.join(PROOF_DIR, 'desktop-map-sheet.png'),
			fullPage: false,
		});
		await page
			.getByRole('button', { name: 'Close Parish Map', exact: true })
			.click();
		await expectCleanFirstViewport(page);
		await expect(section).toHaveAttribute('data-section', 'places');
		await expect(section).toContainText('Places in this Parish');
		await expect(page.getByLabel('Player intent', { exact: true })).toHaveValue(
			'ask Roisin what she saw.',
		);
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
	});

	test('mobile exposes every notebook section and restores it after the Map sheet closes', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);
		const initialBounds = await canvasBounds(page);
		for (const name of [
			'Open Notes notebook tab',
			'Open People notebook tab',
			'Open Places notebook tab',
			'Open Rumours notebook tab',
			'Open Journal notebook tab',
		]) {
			const bounds = await page
				.getByRole('button', { name, exact: true })
				.boundingBox();
			expect(bounds?.width).toBeGreaterThanOrEqual(44);
			expect(bounds?.height).toBeGreaterThanOrEqual(42);
		}

		await activateNotebookControl(page, 'Ask action');
		await expect(
			page.getByRole('textbox', { name: 'Player intent', exact: true }),
		).toHaveValue(/^ask /i);

		const section = page.getByTestId('notebook-active-section');
		for (const tab of [
			'notes',
			'people',
			'places',
			'rumours',
			'journal',
		] as const) {
			const control = await activateNotebookControl(
				page,
				`Open ${tab.charAt(0).toUpperCase()}${tab.slice(1)} notebook tab`,
			);
			await expect(control).toHaveAttribute('aria-pressed', 'true');
			await expect(section).toHaveAttribute('data-section', tab);
			await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(
				0,
			);
			await settleNotebookFrame(page);
			await page.screenshot({
				path: path.join(PROOF_DIR, `mobile-section-${tab}.png`),
				fullPage: false,
			});
		}

		await activateNotebookControl(page, 'Open Places notebook tab');
		const mapInvoker = await activateNotebookControl(page, 'Open parish map');
		const mapOverlay = page.getByRole('dialog', {
			name: 'Parish Map',
			exact: true,
		});
		await expect(mapOverlay).toBeVisible();
		await expect(page.getByLabel('Map controls')).toContainText(
			'click an outlined place to travel',
		);
		await settlePaint(page);
		await page.screenshot({
			path: path.join(PROOF_DIR, 'mobile-map-sheet.png'),
			fullPage: false,
		});
		await page
			.getByRole('button', { name: 'Close Parish Map', exact: true })
			.click();
		await expectCleanFirstViewport(page);
		await expect(mapInvoker).toBeFocused();
		await expect(section).toHaveAttribute('data-section', 'places');
		await expectCanvasBounds(page, initialBounds);
	});

	test('desktop command history and state transitions remain notebook-native', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 1440, height: 900 });
		await setupNotebookPage(page);
		await proveCommandSurface(page, 'desktop');
	});

	test('mobile command history and state transitions remain notebook-native', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await setupNotebookPage(page);
		await proveCommandSurface(page, 'mobile');
	});
});
