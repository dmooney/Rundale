/**
 * E2E proof for slash-command echo rendering (#1423).
 *
 * Verifies that a text-log entry with source:"player" and subtype:"command"
 * renders as a distinct command line (`.entry.command`) above the narration
 * that follows it, NOT as a gold dialogue bubble.
 *
 * Captures a screenshot saved to `.proofs/fix-1423-slash-echo/` as the
 * live-proof artifact.
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';
import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Proof bundle lives at repo-root/.proofs/fix-1423-slash-echo (gitignored).
// __dirname is parish/apps/ui/e2e → up four to the repo root.
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/fix-1423-slash-echo',
);
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

test.describe('slash-command echo rendering (#1423)', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await openJournal(page);
	});

	test('#1423 /pause shows as .entry.command, not a dialogue bubble', async ({
		page,
	}) => {
		const journal = journalOverlay(page);
		const chatPanel = journal.getByTestId('chat-panel');

		// Emit the command echo (source:player, subtype:command)
		await emitEvent(page, 'text-log', {
			id: 'cmd-pause',
			source: 'player',
			subtype: 'command',
			content: '/pause',
		});

		// Followed immediately by the system narration it produced
		await emitEvent(page, 'text-log', {
			id: 'sys-pause',
			source: 'system',
			content: 'The clocks of the parish stand still. Time is now paused.',
		});

		// AC-3: command entry exists with .entry.command
		const commandEntry = chatPanel.getByTestId('command-entry');
		await expect(commandEntry).toBeVisible();
		await expect(commandEntry).toContainText('/pause');

		// AC-3: NOT rendered as a player dialogue bubble
		await expect(chatPanel.locator('.bubble-row.player')).toHaveCount(0);

		// System narration follows below
		const narration = chatPanel
			.locator('.entry.system')
			.filter({ hasText: 'clocks of the parish' });
		await expect(narration).toContainText('clocks of the parish');
		const entries = chatPanel.locator(':scope > .entry, :scope > .bubble-row');
		const lines = await entries.allTextContents();
		const pauseIndex = lines.findIndex((line) => line.includes('/pause'));
		const narrationIndex = lines.findIndex((line) =>
			line.includes('clocks of the parish'),
		);
		expect(pauseIndex).toBeGreaterThan(-1);
		expect(narrationIndex).toBeGreaterThan(-1);
		expect(pauseIndex).toBeLessThan(narrationIndex);

		// Capture proof screenshot
		await expect(journal).toHaveAttribute('data-surface', 'journal');
		fs.mkdirSync(PROOF_DIR, { recursive: true });
		// Keep the full viewport so the artifact shows the command history
		// contained inside the illustrated notebook's Journal sheet.
		await page.screenshot({
			path: path.join(PROOF_DIR, 'command-echo.png'),
			fullPage: false,
		});
	});

	test('#1423 /resume and /wait also render as command entries', async ({
		page,
	}) => {
		const chatPanel = journalOverlay(page).getByTestId('chat-panel');

		await emitEvent(page, 'text-log', {
			id: 'cmd-r',
			source: 'player',
			subtype: 'command',
			content: '/resume',
		});
		await emitEvent(page, 'text-log', {
			id: 'sys-r',
			source: 'system',
			content: 'Time flows once more in the parish.',
		});
		await emitEvent(page, 'text-log', {
			id: 'cmd-w',
			source: 'player',
			subtype: 'command',
			content: '/wait 10',
		});
		await emitEvent(page, 'text-log', {
			id: 'sys-w',
			source: 'system',
			content: 'Ten minutes pass quietly.',
		});

		const commandEntries = chatPanel.getByTestId('command-entry');
		await expect(commandEntries).toHaveCount(2);
		await expect(commandEntries.nth(0)).toContainText('/resume');
		await expect(commandEntries.nth(1)).toContainText('/wait 10');
		// No player bubbles
		await expect(chatPanel.locator('.bubble-row.player')).toHaveCount(0);

		const entries = chatPanel.locator(':scope > .entry, :scope > .bubble-row');
		const lines = await entries.allTextContents();
		const resumeIndex = lines.findIndex((line) => line.includes('/resume'));
		const resumedIndex = lines.findIndex((line) =>
			line.includes('Time flows once more'),
		);
		const waitIndex = lines.findIndex((line) => line.includes('/wait 10'));
		const waitedIndex = lines.findIndex((line) =>
			line.includes('Ten minutes pass'),
		);
		expect(resumeIndex).toBeGreaterThan(-1);
		expect(resumedIndex).toBeGreaterThan(-1);
		expect(waitIndex).toBeGreaterThan(-1);
		expect(waitedIndex).toBeGreaterThan(-1);
		expect(resumeIndex).toBeLessThan(resumedIndex);
		expect(resumedIndex).toBeLessThan(waitIndex);
		expect(waitIndex).toBeLessThan(waitedIndex);
	});
});
