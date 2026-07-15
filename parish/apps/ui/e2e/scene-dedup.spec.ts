/**
 * E2E: location description must not print twice on movement.
 *
 * Regression for the "location printed normally and then again as dialogue"
 * bug. Movement emits BOTH a styled `text-log` entry (subtype `location`, the
 * full arrival scene with exits) AND a `world-update` carrying the shorter
 * `location_description`. The frontend must render the scene once: the
 * world-update append is suppressed when a `location` text-log already showed
 * it. Load/restore (a world-update with no preceding `location` text-log) must
 * still show the destination scene.
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';
import { SNAPSHOTS } from './mock-data';

const PIXI_CANVAS = '[data-testid="illustrated-notebook-pixi-host"] canvas';

function journalOverlay(page: Page) {
	return page.getByRole('dialog', {
		name: 'Parish Journal',
		exact: true,
	});
}

async function openJournal(page: Page) {
	await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
	await expect(page.locator(PIXI_CANVAS)).toBeVisible();
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(
		page.getByRole('button', { name: 'Ask action', exact: true }),
	).toHaveCount(1);

	const control = page.getByRole('button', {
		name: 'Open Journal notebook tab',
		exact: true,
	});
	await expect(control).toHaveCount(1);
	await expect(control).toBeEnabled();
	await control.focus();
	await expect(control).toBeFocused();
	await page.keyboard.press('Enter');

	const journal = journalOverlay(page);
	await expect(journal).toBeVisible();
	await expect(journal).toHaveAttribute('data-surface', 'journal');
	await expect(journal.getByTestId('chat-panel')).toBeVisible();
	return journal;
}

test.describe('Scene description deduplication', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await openJournal(page);
	});

	test('movement renders the arrival scene once, not twice', async ({
		page,
	}) => {
		const chatPanel = journalOverlay(page).getByTestId('chat-panel');

		// Full arrival text the backend sends as a `location` text-log entry.
		const arrivalText =
			'The churchyard lies still beneath a grey sky. Exits: north to the village green.';
		// The shorter, distinct scene line the world-update would otherwise append
		// (render_description without exits / NPC names). Chosen to NOT be a
		// substring of the arrival text so the count is unambiguous.
		const worldUpdateScene =
			'Distinct churchyard scene line from the world snapshot.';

		// Backend ordering on a successful move: `location` text-log, then a
		// world-update for the NEW location.
		await emitEvent(page, 'text-log', {
			source: 'system',
			content: arrivalText,
			subtype: 'location',
		});
		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			location_name: 'Churchyard',
			location_description: worldUpdateScene,
		});

		// The arrival scene shows exactly once.
		await expect(
			chatPanel.getByText(arrivalText, { exact: false }),
		).toHaveCount(1);
		// The duplicate, shorter world-update scene line was suppressed.
		await expect(
			chatPanel.getByText(worldUpdateScene, { exact: false }),
		).toHaveCount(0);
	});

	test('load/restore still shows the destination scene (no location text-log)', async ({
		page,
	}) => {
		const chatPanel = journalOverlay(page).getByTestId('chat-panel');

		// A load/restore world-update changes the location with no preceding
		// `location` text-log — the scene must be shown.
		const loadedScene = 'You stand once more in the loaded harbour town.';
		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.morning,
			location_name: 'Harbour town',
			location_description: loadedScene,
		});

		await expect(
			chatPanel.getByText(loadedScene, { exact: false }),
		).toHaveCount(1);
	});
});
