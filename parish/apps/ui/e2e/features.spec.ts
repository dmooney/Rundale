import {
	test,
	expect,
	installTauriMock,
	emitEvent,
	updateMockResponse,
} from './fixtures';
import { SNAPSHOTS } from './mock-data';

/**
 * Dispatch a KeyboardEvent with the given key.
 * More reliable than page.keyboard.press() for Svelte <svelte:window> handlers.
 */
async function pressKey(page: import('@playwright/test').Page, key: string) {
	await page.evaluate(
		({ key }) => {
			window.dispatchEvent(
				new KeyboardEvent('keydown', { key, bubbles: true }),
			);
		},
		{ key },
	);
}

test.describe('Debug panel', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('opens and shows tab bar on F12', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await expect(debugPanel).toBeVisible();

		const tabButtons = debugPanel.locator('.tab-btn');
		await expect(tabButtons).toHaveCount(8);

		await expect(tabButtons.first()).toHaveClass(/active/);
		await expect(tabButtons.first()).toHaveText('Overview');
	});

	test('navigates between debug tabs', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		const tabButtons = debugPanel.locator('.tab-btn');

		const tabs = [
			'Overview',
			'NPCs',
			'World',
			'Weather',
			'Gossip',
			'Conv',
			'Events',
			'Inference',
		];
		for (let i = 0; i < tabs.length; i++) {
			await tabButtons.nth(i).click();
			await expect(tabButtons.nth(i)).toHaveClass(/active/);
		}
	});

	test('dock toggle and close work', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');

		await debugPanel.locator('.debug-dock-toggle').click();
		await expect(debugPanel).toHaveClass(/left-dock/);

		await debugPanel.locator('.debug-dock-toggle').click();
		await expect(debugPanel).not.toHaveClass(/left-dock/);

		await debugPanel.locator('.debug-close').click();
		await expect(debugPanel).not.toBeVisible();
	});

	test('reopens after close with F12', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await debugPanel.locator('.debug-close').click();
		await expect(debugPanel).not.toBeVisible();

		await pressKey(page, 'F12');
		await expect(debugPanel).toBeVisible();
	});

	test('shows clock data in Overview tab', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await expect(debugPanel).toContainText('08:00');
		await expect(debugPanel).toContainText('Morning | Monday');
		await expect(debugPanel).toContainText('Weather: Clear');
	});

	test('shows weather engine data in Weather tab', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await debugPanel.locator('.tab-btn').nth(3).click();
		await expect(debugPanel).toContainText('Weather Engine');
		await expect(debugPanel).toContainText('Current: Clear');
	});

	test('shows gossip empty state', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await debugPanel.locator('.tab-btn').nth(4).click();
		await expect(debugPanel).toContainText('(no gossip)');
	});

	test('shows conversations empty state', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await debugPanel.locator('.tab-btn').nth(5).click();
		await expect(debugPanel).toContainText('(no exchanges)');
	});

	test('shows world location stats', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await debugPanel.locator('.tab-btn').nth(2).click();
		await expect(debugPanel).toContainText('Locations (1/5 visited)');
	});
});

test.describe('Save picker', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('opens and closes with F5', async ({ page }) => {
		await pressKey(page, 'F5');
		const picker = page.locator('[data-testid="save-picker"]');
		await expect(picker).toBeVisible();
		await expect(picker).toContainText('The Parish Ledger');

		await picker.getByText('Close').click();
		await expect(picker).not.toBeVisible();
	});

	test('can navigate to Ledgers view and back', async ({ page }) => {
		await pressKey(page, 'F5');
		const picker = page.locator('[data-testid="save-picker"]');
		await expect(picker).toBeVisible();

		await picker.getByText('Ledgers').click();
		await expect(picker).toContainText('Ledgers');

		await picker.getByText('Back').click();
		await expect(picker).toContainText('The Parish Ledger');
	});

	test('closes with Escape key', async ({ page }) => {
		await pressKey(page, 'F5');
		const picker = page.locator('[data-testid="save-picker"]');
		await expect(picker).toBeVisible();

		await pressKey(page, 'Escape');
		await expect(picker).not.toBeVisible();
	});

	test('loads a save file when branch is clicked', async ({ page }) => {
		await pressKey(page, 'F5');
		const picker = page.locator('[data-testid="save-picker"]');
		await expect(picker).toBeVisible();

		await expect(picker.locator('.node-body')).toBeVisible({ timeout: 5000 });
		await picker.locator('.node-name').click();
		await expect(picker).not.toBeVisible();
	});

	test('branch and new-game refreshes replace prior narrative context', async ({
		page,
	}) => {
		const chronicle = page.getByLabel('Live chronicle');
		await emitEvent(page, 'text-log', {
			id: 'prior-branch-player',
			source: 'player',
			content: 'Words belonging only to the prior branch.',
		});
		await emitEvent(page, 'text-log', {
			id: 'prior-branch-reply',
			source: 'Old neighbour',
			content: 'A reply belonging only to the prior branch.',
		});
		await expect(chronicle).toContainText('prior branch');

		const loadedScene = {
			...SNAPSHOTS.morning,
			location_id: 15,
			location_name: 'Kilteevan Village',
			location_description: 'The loaded branch opens at the village well.',
		};
		await updateMockResponse(page, 'get_world_snapshot', loadedScene);
		await pressKey(page, 'F5');
		const picker = page.locator('[data-testid="save-picker"]');
		await expect(picker.locator('.node-body')).toBeVisible({ timeout: 5000 });
		await picker.locator('.node-body').click();
		await expect(picker).not.toBeVisible();
		await expect(chronicle).not.toContainText('prior branch');
		await expect(chronicle).toContainText(loadedScene.location_description);

		await emitEvent(page, 'text-log', {
			id: 'loaded-branch-only',
			source: 'player',
			content: 'Words belonging only to the loaded branch.',
		});
		const freshScene = {
			...SNAPSHOTS.morning,
			location_id: 1,
			location_name: 'The Crossroads',
			location_description: 'A fresh game begins at the quiet crossroads.',
		};
		await updateMockResponse(page, 'get_world_snapshot', freshScene);
		await pressKey(page, 'F5');
		await picker.getByText('Ledgers').click();
		await picker.getByText('New Game').click();
		await expect(picker).not.toBeVisible();
		await expect(chronicle).not.toContainText('loaded branch');
		await expect(chronicle).toContainText(freshScene.location_description);
	});
});

test.describe('People notebook', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('people drawer can be opened and closed', async ({ page }) => {
		const peopleControl = page.getByRole('button', {
			name: 'Open People notebook tab',
		});
		await peopleControl.focus();
		await page.keyboard.press('Enter');
		const drawer = page.getByLabel('people drawer');
		await expect(drawer).toBeVisible();

		await drawer.getByRole('button', { name: 'Close notebook drawer' }).click();
		await expect(drawer).not.toBeVisible();

		await peopleControl.focus();
		await page.keyboard.press('Enter');
		await expect(drawer).toBeVisible();
	});

	test('people drawer shows nearby names and occupations', async ({ page }) => {
		const peopleControl = page.getByRole('button', {
			name: 'Open People notebook tab',
		});
		await peopleControl.focus();
		await page.keyboard.press('Enter');
		const drawer = page.getByLabel('people drawer');
		await expect(drawer.getByText('Séamas Ó Briain')).toBeVisible();
		await expect(drawer.getByText('Publican')).toBeVisible();
		await expect(drawer.getByText('Aoife Ní Cheallaigh')).toBeVisible();
		await expect(drawer.getByText('Scholar')).toBeVisible();
	});
});

test.describe('Reactions', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('reaction bar appears for NPC messages with reactions', async ({
		page,
	}) => {
		await emitEvent(page, 'text-log', {
			id: 'npc-msg-1',
			source: 'Séamas Ó Briain',
			content: 'Welcome to the tavern!',
		});

		await emitEvent(page, 'npc-reaction', {
			message_id: 'npc-msg-1',
			emoji: '\u{1F44D}',
			source: 'player',
		});

		await expect(page.getByLabel('Live chronicle')).toContainText(
			'Séamas Ó Briain: Welcome to the tavern!',
		);
		const reactionBar = page
			.locator('.notebook-reaction-strip')
			.getByTestId('reaction-bar');
		await expect(reactionBar).toBeVisible();
		await expect(reactionBar).toContainText('\u{1F44D}');
	});

	test('reaction picker appears on NPC message hover', async ({ page }) => {
		await emitEvent(page, 'text-log', {
			id: 'npc-msg-2',
			source: 'Séamas Ó Briain',
			content: 'Good day to you!',
		});

		const chronicle = page.getByLabel('Live chronicle');
		await expect(chronicle).toContainText('Séamas Ó Briain: Good day to you!');
		const toggle = page.getByRole('button', {
			name: 'React to message from Séamas Ó Briain',
		});
		await expect(toggle).toBeVisible();
		await toggle.hover();

		const picker = page
			.locator('.notebook-reaction-strip')
			.getByTestId('reaction-picker');
		await expect(picker).toBeVisible();
	});
});

test.describe('Editor', () => {
	// NPCs / Locations / Validator tabs only render once an
	// EditorModSnapshot is loaded — see the `{#if snap || t.id === 'mods'
	// || t.id === 'saves'}` guard in parish/apps/ui/src/routes/editor/+page.svelte.
	// The fixture mocks `editor_list_mods` (one ModSummary card) and
	// `editor_open_mod` (a minimal EditorModSnapshot), and these tests click
	// the mod card to load the snapshot before asserting tab state.
	test('navigates to editor and shows tabs', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/editor');
		await page.waitForLoadState('networkidle');

		const editor = page.locator('[data-testid="editor-page"]');
		await expect(editor).toBeVisible();
		await expect(editor).toContainText('Parish Designer');

		await editor.locator('.mod-card').first().click();
		await expect(editor.locator('.tab-btn')).toHaveCount(5);
		await expect(editor.locator('.tab-btn').first()).toHaveText('Mods');
	});

	test('switches between editor tabs', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/editor');
		await page.waitForLoadState('networkidle');

		const editor = page.locator('[data-testid="editor-page"]');
		await editor.locator('.mod-card').first().click();
		await expect(editor.locator('.tab-btn')).toHaveCount(5);

		const labels = ['Mods', 'NPCs', 'Locations', 'Validator', 'Saves'];
		for (const label of labels) {
			await editor.getByText(label).first().click();
			await expect(editor.locator('.tab-btn.active')).toContainText(label);
		}
	});

	test('back link returns to game page', async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/editor');
		await page.waitForLoadState('networkidle');

		await page.locator('.back-link').click();
		await page.waitForLoadState('networkidle');
		await expect(page.getByTestId('illustrated-notebook-game')).toBeVisible();
	});
});
