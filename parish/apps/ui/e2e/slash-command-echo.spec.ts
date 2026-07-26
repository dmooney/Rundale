/**
 * E2E proof for slash-command echo rendering (#1423).
 *
 * Verifies that command echoes and their narration remain ordered on the
 * in-page Journal introduced by #1755. ChatPanel.test.ts retains the detailed
 * `.entry.command` versus dialogue-bubble presentation contract for the
 * reusable legacy component.
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

function journalSection(page: Page) {
	return page.getByTestId('notebook-active-section');
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

	const journal = journalSection(page);
	await expect(journal).toBeVisible();
	await expect(journal).toHaveAttribute('data-section', 'journal');
	await expect(journal).toContainText('Parish Journal');
	await expect(page.getByTestId('notebook-overlay-backdrop')).toHaveCount(0);
	return journal;
}

test.describe('slash-command echo rendering (#1423)', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await openJournal(page);
	});

	test('#1423 /pause remains ordered before its narration', async ({
		page,
	}) => {
		const journal = journalSection(page);

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

		const entries = journal.locator('p');
		await expect(entries.filter({ hasText: '/pause' })).toHaveCount(1);
		await expect(
			entries.filter({ hasText: 'clocks of the parish' }),
		).toHaveCount(1);
		const lines = await entries.allTextContents();
		const pauseIndex = lines.findIndex((line) => line.includes('/pause'));
		const narrationIndex = lines.findIndex((line) =>
			line.includes('clocks of the parish'),
		);
		expect(pauseIndex).toBeGreaterThan(-1);
		expect(narrationIndex).toBeGreaterThan(-1);
		expect(pauseIndex).toBeLessThan(narrationIndex);

		// Capture proof screenshot
		await expect(journal).toHaveAttribute('data-section', 'journal');
		fs.mkdirSync(PROOF_DIR, { recursive: true });
		// Keep the full viewport so the artifact shows the command history
		// contained inside the illustrated notebook's Journal sheet.
		await page.screenshot({
			path: path.join(PROOF_DIR, 'command-echo.png'),
			fullPage: false,
		});
	});

	test('#1423 /resume and /wait remain ordered with their narration', async ({
		page,
	}) => {
		const journal = journalSection(page);

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

		const entries = journal.locator('p');
		await expect(entries.filter({ hasText: '/resume' })).toHaveCount(1);
		await expect(entries.filter({ hasText: '/wait 10' })).toHaveCount(1);
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
