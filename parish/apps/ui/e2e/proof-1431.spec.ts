/**
 * Current-notebook replacements for the retired #1431 dashboard proofs.
 *
 * The illustrated notebook no longer renders the status-bar developer menu,
 * chat bubbles, or scrollable ChatPanel that #1431 originally exercised.
 * These tests preserve the player outcomes on the live surface instead:
 * tools remain reachable and unclipped, action narration remains readable in
 * Journal, the newest journal lines remain present, and People returns cleanly
 * to the Pixi notebook before another drawer opens.
 *
 * Saves PNG proof artifacts to .proofs/1713-notebook-tools/.
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/1713-notebook-tools',
);

test.beforeAll(() => {
	fs.mkdirSync(PROOF_DIR, { recursive: true });
});

async function setupPage(page: import('@playwright/test').Page) {
	await installTauriMock(page, 'morning');
	await page.goto('/');
	await page.waitForLoadState('networkidle');
}

async function activateNotebookControl(
	page: import('@playwright/test').Page,
	name: string,
) {
	const control = page.getByRole('button', { name, exact: true });
	await expect(control).toHaveCount(1);
	await control.focus();
	await expect(control).toBeFocused();
	await page.keyboard.press('Enter');
}

async function openNotebookTools(page: import('@playwright/test').Page) {
	const toggle = page.getByRole('button', { name: 'Notebook tools' });
	await expect(toggle).toBeVisible();
	await toggle.click();

	const drawer = page.locator('aside[aria-label="tools drawer"]');
	await expect(drawer).toBeVisible();
	return drawer;
}

test('tools drawer replaces the retired dev-menu visibility proof', async ({
	page,
}) => {
	await setupPage(page);
	const drawer = await openNotebookTools(page);

	for (const name of ['Save/Load', 'Map', 'Debug', 'Mod', 'Bug Report']) {
		await expect(
			drawer.getByRole('button', { name, exact: true }),
		).toBeVisible();
	}

	const drawerBox = await drawer.boundingBox();
	const viewport = page.viewportSize();
	expect(drawerBox).not.toBeNull();
	expect(viewport).not.toBeNull();
	if (drawerBox && viewport) {
		expect(drawerBox.width).toBeGreaterThan(0);
		expect(drawerBox.height).toBeGreaterThan(0);
		expect(drawerBox.x).toBeGreaterThanOrEqual(0);
		expect(drawerBox.y).toBeGreaterThanOrEqual(0);
		expect(drawerBox.x + drawerBox.width).toBeLessThanOrEqual(viewport.width);
		expect(drawerBox.y + drawerBox.height).toBeLessThanOrEqual(viewport.height);
	}

	await page.screenshot({
		path: path.join(PROOF_DIR, 'tools-drawer-visible.png'),
		fullPage: false,
	});
});

test('action narration remains readable in the Journal drawer', async ({
	page,
}) => {
	await setupPage(page);

	await emitEvent(page, 'text-log', {
		id: 'npc-greeting',
		source: 'Brigid Flanagan',
		content: 'Good morrow to ye.',
	});
	await emitEvent(page, 'text-log', {
		id: 'npc-gesture',
		source: 'a tall stranger',
		content: 'A tall stranger nods silently in your direction.',
		subtype: 'action',
	});

	await activateNotebookControl(page, 'Open Journal notebook tab');
	const journal = page.locator('aside[aria-label="journal drawer"]');
	await expect(journal).toBeVisible();
	await expect(journal).toContainText('Brigid Flanagan');
	await expect(journal).toContainText('Good morrow to ye.');
	const gesture = journal.locator('p').filter({ hasText: 'nods silently' });
	await expect(gesture).toHaveCount(1);
	await expect(gesture).toContainText('a tall stranger');

	await page.screenshot({
		path: path.join(PROOF_DIR, 'journal-action-narration.png'),
		fullPage: false,
	});
});

test('Journal keeps the newest lines in its bounded current window', async ({
	page,
}) => {
	await setupPage(page);

	for (let i = 1; i <= 10; i += 1) {
		await emitEvent(page, 'text-log', {
			id: `journal-entry-${i}`,
			source: i % 2 === 0 ? 'player' : 'NPC',
			content: `Journal message ${i}`,
		});
	}

	await activateNotebookControl(page, 'Open Journal notebook tab');
	const journal = page.locator('aside[aria-label="journal drawer"]');
	await expect(journal).toBeVisible();
	await expect(journal).toContainText('Journal message 10');
	await expect(journal).toContainText('Journal message 3');
	const journalLines = journal.locator('.journal-lines > p');
	await expect(journalLines).toHaveCount(8);
	await expect(
		journalLines.filter({ hasText: /Journal message 1$/ }),
	).toHaveCount(0);

	await page.screenshot({
		path: path.join(PROOF_DIR, 'journal-latest-lines.png'),
		fullPage: false,
	});
});

test('People returns to the notebook before tools reopen', async ({ page }) => {
	await setupPage(page);

	await activateNotebookControl(page, 'Open People notebook tab');
	const people = page.locator('aside[aria-label="people drawer"]');
	await expect(people).toBeVisible();
	await expect(people).toContainText('Séamas Ó Briain');
	await expect(people).toContainText('Aoife Ní Cheallaigh');

	await page.screenshot({
		path: path.join(PROOF_DIR, 'people-drawer.png'),
		fullPage: false,
	});

	await people.getByRole('button', { name: /Aoife Ní Cheallaigh/ }).click();
	await expect(people).not.toBeVisible();
	await expect(
		page.getByTestId('illustrated-notebook-pixi-host'),
	).toBeVisible();
	await expect(
		page.getByTestId('illustrated-notebook-pixi-host').locator('canvas'),
	).toBeVisible();

	const tools = await openNotebookTools(page);
	await expect(tools).toBeVisible();
	await page.screenshot({
		path: path.join(PROOF_DIR, 'people-to-tools.png'),
		fullPage: false,
	});
});
