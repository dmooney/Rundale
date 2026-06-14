/**
 * Proof screenshots for #1431 items 2, 4, and 5.
 *
 * Item 2: NPC gesture / action subtype renders as italic system narration
 *         (entry.subtype === "action" → entryType() returns "system"),
 *         not as a speech bubble.
 *
 * Item 4: Sending a player message auto-scrolls the chat panel to the bottom
 *         so the echoed bubble is always visible.
 *
 * Item 5: The "⋯" developer menu opens fully visible below the status bar —
 *         not clipped by overflow:hidden on .status-bar (now overflow-x:clip).
 *
 * Saves PNG proof artifacts to .proofs/1431-render/ (repo-root relative).
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
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
	// Seed a world-update so the status bar renders with real content.
	await emitEvent(page, 'world-update', {
		location_name: 'Kilteevan Village',
		location_description: 'The small village of Kilteevan.',
		time_label: 'Morning',
		hour: 8,
		minute: 0,
		weather: 'Clear',
		season: 'Spring',
		festival: null,
		paused: false,
		inference_paused: false,
		game_epoch_ms: 0,
		speed_factor: 0,
		name_hints: [],
		day_of_week: 'Monday',
	});
}

// ── Item 5: dev menu visible, not clipped ────────────────────────────────────

test('item5 — dev menu opens below status bar (not clipped)', async ({
	page,
}) => {
	await setupPage(page);

	// Click the ⋯ dev-tools button to open the menu.
	const devToggle = page.getByRole('button', { name: 'Developer tools menu' });
	await expect(devToggle).toBeVisible();
	await devToggle.click();

	const devMenu = page.getByTestId('dev-menu');
	await expect(devMenu).toBeVisible();

	// The menu must be visible with positive dimensions — prior to the fix,
	// overflow:hidden on .status-bar clipped the absolutely-positioned dropdown
	// to zero height and it was rendered but invisible. After the fix
	// (overflow-x:clip + overflow-y:visible) the menu has real painted area.
	const menuBox = await devMenu.boundingBox();
	expect(menuBox).not.toBeNull();
	if (menuBox) {
		expect(menuBox.height).toBeGreaterThan(0);
		expect(menuBox.width).toBeGreaterThan(0);
	}

	// The menu must contain the expected items (Designer, Dbg).
	// The Designer link has role="menuitem"; the Dbg toggle has role="menuitemcheckbox"
	// but we locate it by text to avoid role-resolution quirks in headless Chromium.
	await expect(
		devMenu.getByRole('menuitem', { name: /Designer/i }),
	).toBeVisible();
	await expect(devMenu.locator('text=Dbg')).toBeVisible();

	// The toggle button itself must be inside the status bar.
	const statusBarBox = await page.getByTestId('status-bar').boundingBox();
	const toggleBox = await devToggle.boundingBox();
	expect(statusBarBox).not.toBeNull();
	expect(toggleBox).not.toBeNull();
	if (statusBarBox && toggleBox) {
		// Toggle sits within status bar vertical bounds.
		expect(toggleBox.y).toBeGreaterThanOrEqual(statusBarBox.y - 1);
		expect(toggleBox.y + toggleBox.height).toBeLessThanOrEqual(
			statusBarBox.y + statusBarBox.height + 1,
		);
	}

	// Capture a full-app screenshot showing the menu open and unclipped.
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
		source: 'Brigid Flanagan',
		content: 'Good morrow to ye.',
	});

	// Now seed a gesture/action line with subtype "action".
	await emitEvent(page, 'text-log', {
		id: 'npc-gesture',
		source: 'a tall stranger',
		content: 'A tall stranger nods silently in your direction.',
		subtype: 'action',
	});

	// The greeting must be a speech bubble.
	const greetingBubble = page.locator('.bubble-row.npc').first();
	await expect(greetingBubble).toBeVisible();

	// The gesture must render as a system entry, not a bubble.
	const systemEntries = page.locator('.entry.system');
	// There should be at least one system entry (gesture + any splash).
	await expect(systemEntries.last()).toBeVisible();

	// The gesture text must appear in a system entry, NOT in an NPC bubble.
	await expect(
		page.locator('.entry.system').filter({ hasText: 'nods silently' }),
	).toBeVisible();
	await expect(
		page.locator('.bubble-row.npc').filter({ hasText: 'nods silently' }),
	).toHaveCount(0);

	await page.screenshot({
		path: path.join(PROOF_DIR, 'item2-gesture-as-system.png'),
		fullPage: false,
	});
});

// ── Item 4: player submit auto-scrolls chat to bottom ────────────────────────

test('item4 — submitting a message scrolls chat to bottom', async ({
	page,
}) => {
	await setupPage(page);

	// Fill the chat with enough entries to make it scrollable.
	for (let i = 1; i <= 20; i++) {
		await emitEvent(page, 'text-log', {
			id: `entry-${i}`,
			source: i % 3 === 0 ? 'player' : 'NPC',
			content: `Message number ${i} — filling the chat log to ensure it overflows.`,
		});
	}

	// Wait for entries to render, then scroll the panel to the top to simulate
	// a user who has scrolled up to read back through history.
	const chatPanel = page.getByTestId('chat-panel');
	await page.waitForTimeout(100);
	await chatPanel.evaluate((el) => {
		el.scrollTop = 0;
	});

	// Confirm we are NOT at the bottom before submitting.
	const scrolledAway = await chatPanel.evaluate((el) => el.scrollTop === 0);
	expect(scrolledAway).toBe(true);

	// Type a message and press Enter — this triggers handleSubmit in InputField,
	// which increments playerSubmittedCount before the backend echo arrives.
	// The submit_input mock call returns null (no-op in the Tauri mock), so the
	// IPC round-trip completes silently. We then emit the player echo manually
	// (as the backend would) to drive the text-log update that triggers the
	// ChatPanel $effect scroll.
	const inputField = page.getByTestId('input-field');
	await inputField.click();
	await inputField.fill('What shall I do here?');
	await inputField.press('Enter');

	// Emit the backend echo that normally arrives after submit_input returns.
	await emitEvent(page, 'text-log', {
		id: 'player-submit',
		source: 'player',
		content: 'What shall I do here?',
	});

	// Allow the Svelte tick + scroll effect to settle.
	await page.waitForTimeout(200);

	// The chat panel must now be scrolled to the bottom — the player-submitted
	// flag bypasses the near-bottom guard (#1431 item 4).
	const isAtBottom = await chatPanel.evaluate((el) => {
		const delta = el.scrollHeight - el.scrollTop - el.clientHeight;
		return delta < 10;
	});
	expect(isAtBottom).toBe(true);

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

	// Seed a regular NPC bubble for contrast.
	await emitEvent(page, 'text-log', {
		id: 'npc-greeting-combined',
		source: 'Brigid Flanagan',
		content: 'Good morrow to ye.',
	});

	// Open the dev menu.
	await page.getByRole('button', { name: 'Developer tools menu' }).click();
	await expect(page.getByTestId('dev-menu')).toBeVisible();

	await page.screenshot({
		path: path.join(PROOF_DIR, 'combined-proof.png'),
		fullPage: false,
	});
});
