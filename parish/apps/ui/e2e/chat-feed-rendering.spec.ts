/**
 * E2E proof for readable player-log content in the illustrated notebook.
 *
 * The current Journal deliberately replaces the retired rich-chat bubble CSS.
 * These checks preserve the original full-destination and message-order
 * outcomes through that surface. Rich reaction presentation remains tracked by
 * #1630 instead of reintroducing compatibility DOM solely for old selectors.
 */

import type { Page } from '@playwright/test';
import { test, expect, installTauriMock, emitEvent } from './fixtures';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Proof bundle lives at repo-root/.proofs/1712-notebook-journal-e2e (gitignored).
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

test.describe('chat-feed rendering (#1226, #1275)', () => {
	test.beforeEach(async ({ page }) => {
		// Default light theme — gold player bubble, cream page — matches the bug.
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		// Current location drives `.term-location` highlighting of the
		// destination in the go-to echo bubble.
		await emitEvent(page, 'world-update', {
			location_name: 'The Crossroads',
			location_description: 'A quiet crossroads where four narrow roads meet.',
			time_label: 'Dusk',
			hour: 18,
			minute: 59,
			weather: 'Partly Cloudy',
			season: 'Spring',
			festival: null,
			paused: false,
			inference_paused: false,
			game_epoch_ms: 0,
			speed_factor: 0,
			name_hints: [],
			day_of_week: 'Tuesday',
		});
	});

	test('#1226 Journal shows the full movement destination', async ({
		page,
	}) => {
		const journal = await openJournal(page);
		// Backend echoes "> go to <dest>"; the controller strips "> ".
		await emitEvent(page, 'text-log', {
			id: 'p-goto',
			source: 'player',
			content: '> go to The Crossroads',
		});

		const movementLine = journalLine(journal, 'go to The Crossroads');
		await expect(movementLine).toHaveCount(1);
		await expect(movementLine).toContainText('player: go to The Crossroads');
	});

	test('#1275 reaction events preserve readable Journal content and order (#1630)', async ({
		page,
	}) => {
		const journal = await openJournal(page);
		// Player message several co-located NPCs will react to.
		await emitEvent(page, 'text-log', {
			id: 'p-react',
			source: 'player',
			content: '> there is not',
		});
		const reactors = [
			{ emoji: '🤔', source: 'Mick Flanagan' },
			{ emoji: '😏', source: 'Brendan Duffy' },
			{ emoji: '🙄', source: 'Padraig Darcy' },
			{ emoji: '😳', source: 'Fr. Declan Tierney' },
			{ emoji: '👀', source: 'Roisin Connolly' },
		];
		for (const r of reactors) {
			await emitEvent(page, 'npc-reaction', {
				message_id: 'p-react',
				emoji: r.emoji,
				source: r.source,
			});
		}
		await emitEvent(page, 'text-log', {
			id: 'sys-after-reactions',
			source: 'system',
			content: 'The parish watches in thoughtful silence.',
		});

		const playerLine = journalLine(journal, 'there is not');
		const followingLine = journalLine(
			journal,
			'The parish watches in thoughtful silence.',
		);
		await expect(playerLine).toHaveCount(1);
		await expect(playerLine).toContainText('player: there is not');
		await expect(followingLine).toHaveCount(1);
		const lines = await journal.locator('p').allTextContents();
		const playerIdx = lines.findIndex((line) => line.includes('there is not'));
		const systemIdx = lines.findIndex((line) =>
			line.includes('thoughtful silence'),
		);
		expect(playerIdx).toBeGreaterThan(-1);
		expect(systemIdx).toBeGreaterThan(-1);
		expect(playerIdx).toBeLessThan(systemIdx);
	});

	test('capture current Journal proof (#1226 + #1275 migration)', async ({
		page,
	}) => {
		const journal = await openJournal(page);
		// Movement line with full destination (#1226).
		await emitEvent(page, 'text-log', {
			id: 'p-goto',
			source: 'player',
			content: '> go to The Crossroads',
		});
		// Player message with multiple NPC reactions (#1275).
		await emitEvent(page, 'text-log', {
			id: 'p-react',
			source: 'player',
			content: '> there is not',
		});
		for (const r of [
			{ emoji: '🤔', source: 'Mick Flanagan' },
			{ emoji: '😏', source: 'Brendan Duffy' },
			{ emoji: '🙄', source: 'Padraig Darcy' },
			{ emoji: '😳', source: 'Fr. Declan Tierney' },
			{ emoji: '👀', source: 'Roisin Connolly' },
		]) {
			await emitEvent(page, 'npc-reaction', {
				message_id: 'p-react',
				emoji: r.emoji,
				source: r.source,
			});
		}
		await emitEvent(page, 'text-log', {
			id: 'sys-after-reactions',
			source: 'system',
			content: 'The parish watches in thoughtful silence.',
		});

		await expect(journalLine(journal, 'go to The Crossroads')).toContainText(
			'player: go to The Crossroads',
		);
		await expect(journalLine(journal, 'there is not')).toContainText(
			'player: there is not',
		);
		await expect(journalLine(journal, 'thoughtful silence')).toHaveCount(1);

		fs.mkdirSync(PROOF_DIR, { recursive: true });
		await page.screenshot({
			path: path.join(PROOF_DIR, 'journal-content-order.png'),
			fullPage: false,
		});
	});
});
