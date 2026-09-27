/**
 * Default-route contracts for the chat-first player shell.
 */

import { expect, test, installTauriMock, emitEvent } from './fixtures';
import { NPCS, SNAPSHOTS } from './mock-data';

async function waitForChat(page: import('@playwright/test').Page) {
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(page.getByTestId('chat-game-shell')).toBeVisible();
	await expect(page.getByTestId('input-field')).toBeEditable();
}

test.describe('Chat-first app layout', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await waitForChat(page);
	});

	test('renders the mature chat shell as the default route', async ({
		page,
	}) => {
		await expect(page.getByTestId('status-bar')).toBeVisible();
		await expect(page.getByTestId('chat-panel')).toBeVisible();
		await expect(page.getByTestId('input-field')).toBeVisible();
		await expect(page.getByTestId('sidebar')).toBeVisible();
		await expect(page.getByTestId('map-panel')).toBeVisible();
		await expect(page.locator('canvas')).toHaveCount(1);
		await expect(page.getByTestId('surface-backdrop')).toHaveCount(0);
	});

	test('shows the current place, time context, people, and language hints', async ({
		page,
	}) => {
		const status = page.getByTestId('status-bar');
		await expect(status).toContainText(SNAPSHOTS.morning.location_name);
		await expect(status).toContainText(SNAPSHOTS.morning.weather);
		for (const npc of NPCS) {
			await expect(page.getByTestId('sidebar')).toContainText(npc.name);
		}
		await expect(page.getByTestId('sidebar')).toContainText('[EE-fa]');
	});

	test('the native input is visible, editable, and submits a command', async ({
		page,
	}) => {
		const input = page.getByTestId('input-field');
		await input.fill('ask what happened');
		await input.press('Enter');
		await expect(input).toHaveText('');
		await expect
			.poll(() =>
				page.evaluate(() =>
					(
						window as unknown as {
							__TEST_INVOKE_CALLS__: Array<{
								command: string;
								args?: Record<string, unknown>;
							}>;
						}
					).__TEST_INVOKE_CALLS__.find(
						(call) => call.command === 'submit_input',
					),
				),
			)
			.toMatchObject({
				command: 'submit_input',
				args: { text: 'ask what happened', addressedTo: [] },
			});
	});

	test('opens one coordinated surface at a time and restores focus', async ({
		page,
	}) => {
		const ledger = page.getByRole('button', { name: 'Save/Load picker' });
		await ledger.focus();
		await ledger.click();
		await expect(page.getByTestId('surface-save')).toBeVisible();

		await page.keyboard.press('F12');
		await expect(page.getByTestId('surface-save')).toHaveCount(0);
		const debug = page.getByTestId('surface-debug');
		await expect(debug).toBeVisible();
		await expect(page.getByRole('dialog')).toHaveCount(1);

		await page.keyboard.press('Escape');
		await expect(debug).toHaveCount(0);
	});

	test('routes map and shortcuts hotkeys outside text entry', async ({
		page,
	}) => {
		await page.getByTestId('status-bar').click();
		await page.keyboard.press('m');
		await expect(page.getByTestId('surface-map')).toBeVisible();
		await page.keyboard.press('Escape');

		await page.evaluate(() => (document.activeElement as HTMLElement)?.blur());
		await page.evaluate(() =>
			window.dispatchEvent(
				new KeyboardEvent('keydown', { key: '?', bubbles: true }),
			),
		);
		await expect(page.getByTestId('surface-shortcuts')).toBeVisible();
	});

	test('does not steal single-letter shortcuts while the player is typing', async ({
		page,
	}) => {
		const input = page.getByTestId('input-field');
		await input.focus();
		await input.fill('m?');
		await expect(page.getByTestId('surface-backdrop')).toHaveCount(0);
		await expect(input).toHaveText('m?');
	});

	test('backend map and save events reach visible destinations', async ({
		page,
	}) => {
		await emitEvent(page, 'toggle-full-map');
		await expect(page.getByTestId('surface-map')).toBeVisible();
		await emitEvent(page, 'save-picker');
		await expect(page.getByTestId('surface-map')).toHaveCount(0);
		await expect(page.getByTestId('surface-save')).toBeVisible();
	});

	test('mobile keeps chat and input reachable and exposes panel controls', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await expect(page.getByTestId('chat-panel')).toBeVisible();
		await expect(page.getByTestId('input-field')).toBeVisible();
		await expect(
			page.getByRole('button', { name: 'Toggle parish map' }),
		).toBeVisible();
		const people = page.getByRole('button', {
			name: 'Toggle nearby people and language hints',
		});
		await expect(people).toBeVisible();
		await people.click();
		const mobilePeople = page.getByTestId('mobile-people-panel');
		await expect(mobilePeople).toBeVisible();
		await expect(mobilePeople.getByTestId('npcs-present')).toContainText(
			NPCS[0].name,
		);
		await expect(mobilePeople.getByText('[EE-fa]')).toBeVisible();
		const overflow = await page.evaluate(
			() => document.documentElement.scrollWidth > window.innerWidth,
		);
		expect(overflow).toBe(false);
	});
});
