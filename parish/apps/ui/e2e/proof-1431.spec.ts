/**
 * Proof screenshots for #1431 items 2, 4, and 5.
 *
 * Item 2: NPC gesture / action subtype renders as Parish narration in the
 *         illustrated notebook chronicle, not as NPC speech.
 *
 * Item 4: Sending a player message keeps the echoed line in the bounded Pixi
 *         chronicle, even after enough output to exceed its line budget.
 *
 * Item 5: The notebook tools drawer opens fully visible inside the viewport.
 *
 * Saves PNG proof artifacts to .proofs/1431-render/ (repo-root relative).
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import { SNAPSHOTS } from './mock-data';
import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// __dirname is parish/apps/ui/e2e → up four levels to repo root
const PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/1431-render');

test.beforeAll(() => {
	fs.mkdirSync(PROOF_DIR, { recursive: true });
});

// ── Shared setup ─────────────────────────────────────────────────────────────

async function setupPage(page: import('@playwright/test').Page) {
	await installTauriMock(page, 'morning');
	await page.goto('/');
	await page.waitForLoadState('networkidle');
	// Seed a valid world update so the notebook renders with real content.
	await emitEvent(page, 'world-update', {
		...SNAPSHOTS.morning,
		location_id: 15,
		location_name: 'Kilteevan Village',
		location_description: 'The small village of Kilteevan.',
	});
}

// ── Item 5: tools drawer visible, not clipped ────────────────────────────────

test('item5 — notebook tools drawer opens inside the viewport', async ({
	page,
}) => {
	await setupPage(page);

	const toolsToggle = page.getByRole('button', { name: 'Notebook tools' });
	await expect(toolsToggle).toBeVisible();
	await toolsToggle.click();

	const toolsDrawer = page.getByLabel('tools drawer');
	await expect(toolsDrawer).toBeVisible();

	const drawerBox = await toolsDrawer.boundingBox();
	expect(drawerBox).not.toBeNull();
	if (drawerBox) {
		expect(drawerBox.height).toBeGreaterThan(0);
		expect(drawerBox.width).toBeGreaterThan(0);
		expect(drawerBox.x).toBeGreaterThanOrEqual(0);
		expect(drawerBox.y).toBeGreaterThanOrEqual(0);
		expect(drawerBox.x + drawerBox.width).toBeLessThanOrEqual(
			page.viewportSize()?.width ?? Number.POSITIVE_INFINITY,
		);
		expect(drawerBox.y + drawerBox.height).toBeLessThanOrEqual(
			page.viewportSize()?.height ?? Number.POSITIVE_INFINITY,
		);
	}

	for (const label of ['Save/Load', 'Map', 'Debug', 'Mod', 'Bug Report']) {
		await expect(
			toolsDrawer.getByRole('button', { name: label }),
		).toBeVisible();
	}

	const notebookBox = await page
		.getByTestId('illustrated-notebook-game')
		.boundingBox();
	const toggleBox = await toolsToggle.boundingBox();
	expect(notebookBox).not.toBeNull();
	expect(toggleBox).not.toBeNull();
	if (notebookBox && toggleBox) {
		expect(toggleBox.y).toBeGreaterThanOrEqual(notebookBox.y - 1);
		expect(toggleBox.y + toggleBox.height).toBeLessThanOrEqual(
			notebookBox.y + notebookBox.height + 1,
		);
	}

	await page.screenshot({
		path: path.join(PROOF_DIR, 'item5-dev-menu-visible.png'),
		fullPage: false,
	});
});

// ── Item 2: gesture/action subtype → system narration, not speech bubble ─────

test('item2 — action subtype renders as system narration, not NPC bubble', async ({
	page,
}) => {
	await setupPage(page);

	// Seed a regular NPC speech bubble first (regression guard).
	await emitEvent(page, 'text-log', {
		id: 'npc-greeting',
		source: 'Séamas Ó Briain',
		content: 'Good morrow to ye.',
	});

	// Now seed a gesture/action line with subtype "action".
	await emitEvent(page, 'text-log', {
		id: 'npc-gesture',
		source: 'a tall stranger',
		content: 'A tall stranger nods silently in your direction.',
		subtype: 'action',
	});

	const chronicle = page.getByLabel('Live chronicle');
	await expect(chronicle).toContainText('Séamas Ó Briain: Good morrow to ye.');
	await expect(chronicle).toContainText(
		'Parish: A tall stranger nods silently in your direction.',
	);
	await expect(chronicle).not.toContainText(
		'Someone: A tall stranger nods silently in your direction.',
	);

	await page.screenshot({
		path: path.join(PROOF_DIR, 'item2-gesture-as-system.png'),
		fullPage: false,
	});
});

// ── Item 4: player submit remains in bounded live chronicle ─────────────────

test('item4 — submitting a message keeps its echo in the live chronicle', async ({
	page,
}) => {
	await setupPage(page);

	// Fill the transcript past the notebook's fixed line budget.
	for (let i = 1; i <= 20; i++) {
		await emitEvent(page, 'text-log', {
			id: `entry-${i}`,
			source: i % 3 === 0 ? 'player' : 'NPC',
			content: `Message number ${i} — filling the chat log to ensure it overflows.`,
		});
	}

	const inputField = page.getByLabel('Player intent');
	await inputField.fill('What shall I do here?');
	await inputField.press('Enter');

	await emitEvent(page, 'text-log', {
		id: 'player-submit',
		source: 'player',
		content: 'What shall I do here?',
	});

	await expect(page.getByLabel('Live chronicle')).toContainText(
		'You: What shall I do here?',
	);
	await expect(
		page.getByTestId('illustrated-notebook-pixi-host'),
	).toHaveAttribute('data-visible-live-line-keys', /player-submit/);

	await page.screenshot({
		path: path.join(PROOF_DIR, 'item4-auto-scroll.png'),
		fullPage: false,
	});
});

// ── Combined proof screenshot ─────────────────────────────────────────────────

test('combined — action narration + dev menu visible in one view', async ({
	page,
}) => {
	await setupPage(page);

	// Seed a gesture entry so the system narration is visible in the chat.
	await emitEvent(page, 'text-log', {
		id: 'npc-gesture-combined',
		source: 'a tall stranger',
		content: 'A tall stranger nods silently in your direction.',
		subtype: 'action',
	});

	// Seed a regular NPC line for contrast.
	await emitEvent(page, 'text-log', {
		id: 'npc-greeting-combined',
		source: 'Séamas Ó Briain',
		content: 'Good morrow to ye.',
	});

	await page.getByRole('button', { name: 'Notebook tools' }).click();
	await expect(page.getByLabel('tools drawer')).toBeVisible();

	await page.screenshot({
		path: path.join(PROOF_DIR, 'combined-proof.png'),
		fullPage: false,
	});
});
