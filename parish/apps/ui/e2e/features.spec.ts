import { test, expect, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';

/**
 * Dispatch a KeyboardEvent with the given key.
 * More reliable than page.keyboard.press() for Svelte <svelte:window> handlers.
 */
async function pressKey(page: import('@playwright/test').Page, key: string) {
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await page.evaluate(
		({ key }) => {
			window.dispatchEvent(
				new KeyboardEvent('keydown', { key, bubbles: true }),
			);
		},
		{ key },
	);
}

async function activateNotebookControl(page: Page, name: string) {
	await expect(page.locator('.app-shell')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	const control = page.getByRole('button', { name, exact: true });
	await expect(control).toHaveCount(1);
	await control.focus();
	await expect(control).toBeFocused();
	await page.keyboard.press('Enter');
	return control;
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

	test('notebook wrapper and close work', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		const wrapper = page.getByRole('dialog', { name: 'Parish Records' });
		await expect(wrapper).toBeVisible();
		await expect(debugPanel.locator('.debug-dock-toggle')).toBeHidden();

		await wrapper.getByRole('button', { name: 'Close Parish Records' }).click();
		await expect(debugPanel).not.toBeVisible();
	});

	test('reopens after close with F12', async ({ page }) => {
		await pressKey(page, 'F12');
		const debugPanel = page.locator('[data-testid="debug-panel"]');
		await page.getByRole('button', { name: 'Close Parish Records' }).click();
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
});

test.describe('Focail notebook overlay', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('opens from notebook tools and closes back to the clean parish view', async ({
		page,
	}) => {
		await expect(page.getByTestId('sidebar')).toHaveCount(0);
		const focail = await openFocail(page);
		await expect(focail.locator('.focail-panel')).toBeVisible();

		await focail
			.getByRole('button', { name: 'Close Focail — Irish Words' })
			.click();
		await expect(focail).not.toBeVisible();
		await expect(page.getByTestId('illustrated-notebook-game')).toHaveAttribute(
			'aria-hidden',
			'false',
		);
	});

	test('shows language hints when opened', async ({ page }) => {
		const focail = await openFocail(page);
		await expect(focail.getByText('Baile Átha Cliath')).toBeVisible();
		await expect(focail.getByText('[EE-fa]')).toBeVisible();
	});
});

// #1755 retired the modal ChatPanel route from the player-facing Journal tab.
// These two browser tests exercise controls that only exist inside that legacy
// component; ChatPanel.test.ts retains the picker, IPC, and reaction-bar
// contracts until reactions receive an in-page notebook interaction design.
test.describe.skip('Legacy ChatPanel reactions', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('reaction bar appears for NPC messages with reactions', async ({
		page,
	}) => {
		const journal = await openJournal(page);
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

		const reactionBar = journal.locator('[data-testid="reaction-bar"]');
		await expect(reactionBar).toBeVisible();
		await expect(reactionBar).toContainText('\u{1F44D}');
	});

	test('reaction picker appears on NPC message hover', async ({ page }) => {
		const journal = await openJournal(page);
		await emitEvent(page, 'text-log', {
			id: 'npc-msg-2',
			source: 'Séamas Ó Briain',
			content: 'Good day to you!',
		});

		const bubbleAnchor = journal.locator('.bubble-anchor').first();
		await bubbleAnchor.hover();

		const picker = journal.locator('[data-testid="reaction-picker"]');
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
		await expect(
			page.getByRole('button', { name: 'Ask action', exact: true }),
		).toHaveCount(1);
	});
});
