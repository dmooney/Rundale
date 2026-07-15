/**
 * E2E proof for slash-command echo ordering in the illustrated notebook.
 *
 * The Journal replaces the retired chat-entry CSS. These checks keep command
 * text, source attribution, and command-before-result ordering readable without
 * adding compatibility DOM solely for the old selectors.
 */

import type { Page } from '@playwright/test';
import { test, expect, installTauriMock, emitEvent } from './fixtures';
import * as path from 'path';
import * as fs from 'fs';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Proof bundle lives at repo-root/.proofs/fix-1423-slash-echo (gitignored).
// __dirname is parish/apps/ui/e2e → up four to the repo root.
const PROOF_DIR = path.resolve(
	__dirname,
	'../../../../.proofs/1712-notebook-journal-e2e',
);

async function openJournal(page: Page) {
	const trigger = page.getByRole('button', {
		name: 'Open Journal notebook tab',
	});
	await expect(trigger).toBeVisible();
	await trigger.focus();
	await page.keyboard.press('Enter');
	const journal = page.getByLabel('journal drawer');
	await expect(journal).toBeVisible();
	return journal;
}

function journalLine(journal: ReturnType<Page['locator']>, text: string) {
	return journal.locator('p').filter({ hasText: text });
}

test.describe('slash-command echo rendering (#1423)', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('#1423 /pause remains readable before its Journal result', async ({
		page,
	}) => {
		const journal = await openJournal(page);
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

		const commandEntry = journalLine(journal, '/pause');
		const narration = journalLine(journal, 'clocks of the parish');
		await expect(commandEntry).toHaveCount(1);
		await expect(commandEntry).toContainText('player: /pause');
		await expect(narration).toContainText(
			'system: The clocks of the parish stand still. Time is now paused.',
		);
		const lines = await journal.locator('p').allTextContents();
		const pauseIdx = lines.findIndex((line) => line.includes('/pause'));
		const clocksIdx = lines.findIndex((line) =>
			line.includes('clocks of the parish'),
		);
		expect(pauseIdx).toBeGreaterThan(-1);
		expect(clocksIdx).toBeGreaterThan(-1);
		expect(pauseIdx).toBeLessThan(clocksIdx);

		// Capture proof screenshot
		fs.mkdirSync(PROOF_DIR, { recursive: true });
		await page.screenshot({
			path: path.join(PROOF_DIR, 'command-echo.png'),
			fullPage: false,
		});
	});

	test('#1423 /resume and /wait also render as command entries', async ({
		page,
	}) => {
		const journal = await openJournal(page);
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

		await expect(journalLine(journal, '/resume')).toContainText(
			'player: /resume',
		);
		await expect(journalLine(journal, 'Time flows once more')).toContainText(
			'system: Time flows once more in the parish.',
		);
		await expect(journalLine(journal, '/wait 10')).toContainText(
			'player: /wait 10',
		);
		await expect(journalLine(journal, 'Ten minutes pass')).toContainText(
			'system: Ten minutes pass quietly.',
		);

		const lines = await journal.locator('p').allTextContents();
		const resume = lines.findIndex((line) => line.includes('/resume'));
		const resumed = lines.findIndex((line) => line.includes('Time flows'));
		const wait = lines.findIndex((line) => line.includes('/wait 10'));
		const waited = lines.findIndex((line) => line.includes('Ten minutes'));
		expect(resume).toBeGreaterThan(-1);
		expect(resumed).toBeGreaterThan(-1);
		expect(wait).toBeGreaterThan(-1);
		expect(waited).toBeGreaterThan(-1);
		expect(resume).toBeLessThan(resumed);
		expect(resumed).toBeLessThan(wait);
		expect(wait).toBeLessThan(waited);
	});
});
