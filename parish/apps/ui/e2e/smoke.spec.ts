import { test, expect, type Page } from '@playwright/test';
import {
	installTileRouteMock,
	waitForTextureCompleteNotebookFrame,
} from './fixtures';

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';

async function waitForNotebook(page: Page): Promise<void> {
	const shell = page.locator('.app-shell');
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible({
		timeout: 10_000,
	});
	await expect(page.locator(PIXI_CANVAS)).toBeVisible();
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);
	// The real-server suite submits immediately after this helper. Waiting for
	// the page controller prevents a cold-start race before WebSocket/Tauri
	// listeners have finished registering.
	await expect(shell).toHaveAttribute('data-controller-ready', 'true');
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
	const journal = page.getByTestId('notebook-active-section');
	await expect(journal).toBeVisible();
	await expect(journal).toHaveAttribute('data-section', 'journal');
	await expect(journal).toContainText('Parish Journal');
	await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);
	return journal;
}

async function closeNotebookSurface(page: Page, title: string): Promise<void> {
	await page
		.getByRole('button', { name: `Close ${title}`, exact: true })
		.click();
	await expect(
		page.getByRole('dialog', { name: title, exact: true }),
	).toHaveCount(0);
}

async function submitIntent(page: Page, text: string): Promise<void> {
	const input = page.getByLabel('Player intent', { exact: true });
	await input.focus();
	await expect(input).toBeFocused();
	await page.keyboard.insertText(text);
	await expect(input).toHaveValue(text);
	await page.keyboard.press('Enter');
	await expect(input).toHaveValue('', { timeout: 30_000 });
}

test.describe('Parish Web UI', () => {
	test.beforeEach(async ({ page }) => {
		await installTileRouteMock(page);
	});

	test('page loads with game state', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		// The first viewport is the Pixi notebook scene; legacy dashboard
		// surfaces are mounted only after an explicit notebook action.
		await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);
		await expect(page.getByTestId('status-bar')).toHaveCount(0);
		await expect(page.getByTestId('chat-panel')).toHaveCount(0);
		await expect(page.getByTestId('sidebar')).toHaveCount(0);
		await expect(page.getByTestId('full-map')).toHaveCount(0);
		await expect(
			page.getByLabel('Player intent', { exact: true }),
		).toBeEnabled();

		const journal = await openJournal(page);
		await expect(journal.locator('p')).not.toHaveCount(0);

		await activateNotebookControl(page, 'Open People notebook tab');
		const people = page.getByTestId('notebook-active-section');
		await expect(people).toBeVisible();
		await expect(people).toHaveAttribute('data-section', 'people');
		await expect(people).toContainText('Nearby');
		await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);

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
		await expect(focail.locator('.focail-panel')).toBeVisible();
		await closeNotebookSurface(page, 'Focail — Irish Words');

		await activateNotebookControl(page, 'Open parish map');
		const map = page.getByRole('dialog', {
			name: 'Parish Map',
			exact: true,
		});
		await expect(map).toBeVisible();
		await expect(
			map.getByTestId('full-map').locator('canvas.maplibregl-canvas'),
		).toBeVisible();
	});

	test('player can type a command', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		const snapshotResponse = await page.request.get('/api/world-snapshot');
		expect(snapshotResponse.ok()).toBeTruthy();
		const snapshot = (await snapshotResponse.json()) as {
			location_description: string;
		};

		await submitIntent(page, 'look');

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
		const journal = await openJournal(page);
		await expect(
			journal.locator('p').filter({ hasText: snapshot.location_description }),
		).toHaveCount(2, { timeout: 30_000 });
		await expect(journal.getByText('look', { exact: true })).toHaveCount(0);
	});

	test('player can move to a location', async ({ page }) => {
		await page.goto('/');
		await waitForNotebook(page);

		const mapResponse = await page.request.get('/api/map');
		expect(mapResponse.ok()).toBeTruthy();
		const mapData = (await mapResponse.json()) as {
			locations: Array<{ name: string; adjacent: boolean }>;
		};
		const destination = mapData.locations.find((location) => location.adjacent);
		expect(destination).toBeTruthy();
		if (!destination) return;

		await submitIntent(page, `go to ${destination.name}`);
		await expect
			.poll(
				async () => {
					const response = await page.request.get('/api/world-snapshot');
					const snapshot = (await response.json()) as {
						location_name: string;
					};
					return snapshot.location_name;
				},
				{ timeout: 30_000 },
			)
			.toBe(destination.name);

		const journal = await openJournal(page);
		await expect(journal).toContainText(destination.name, {
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
		await waitForNotebook(page);

		// Capture the clean illustrated first viewport before opening any sheet.
		await waitForTextureCompleteNotebookFrame(page);
		await page.screenshot({
			path: 'e2e-results/initial-load.png',
			fullPage: false,
		});

		// After a command
		await submitIntent(page, '/status');
		const journal = await openJournal(page);
		await expect(journal).toContainText('Location:', {
			timeout: 30_000,
		});
		await waitForTextureCompleteNotebookFrame(page);
		await page.screenshot({
			path: 'e2e-results/after-status.png',
			fullPage: false,
		});
	});
});
