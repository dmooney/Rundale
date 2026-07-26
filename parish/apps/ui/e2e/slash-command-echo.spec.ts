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

test.describe('slash-command echo rendering (#1423)', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('#1423 /pause shows as .entry.command, not a dialogue bubble', async ({
		page,
	}) => {
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

		// AC-3: command entry remains distinct in the notebook chronicle.
		const commandEntry = page.locator('[data-testid="command-entry"]');
		await expect(commandEntry).toBeAttached();
		await expect(commandEntry).toContainText('/pause');
		await expect(commandEntry).toContainText('Command:');

		// AC-3: NOT rendered as a player dialogue bubble.
		await expect(page.locator('.bubble-row.player')).toHaveCount(0);

		// System narration follows in the accessible live chronicle.
		const narration = page
			.getByLabel('Live chronicle')
			.getByText(/clocks of the parish/);
		await expect(narration).toContainText('clocks of the parish');

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

		const commandEntries = page.locator('[data-testid="command-entry"]');
		await expect(commandEntries).toHaveCount(2);
		await expect(commandEntries.nth(0)).toContainText('/resume');
		await expect(commandEntries.nth(1)).toContainText('/wait 10');
		// No player bubbles
		await expect(page.locator('.bubble-row.player')).toHaveCount(0);
	});
});
