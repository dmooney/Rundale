/**
 * E2E proof for the chat-feed rendering fixes:
 *
 *  - #1226: a go-to / movement player echo must show the FULL destination in
 *    the illustrated notebook chronicle. The old bubble's term highlighting
 *    no longer exists, so the regression is asserted at the authoritative
 *    accessible transcript and Pixi line-selection seam.
 *
 *  - #1275: when several co-located NPCs react to one player message, the
 *    reaction chips must wrap cleanly and stay aligned under the (right-
 *    aligned) player bubble — not spill to the left of it.
 *
 * Uses the default cream/gold light theme (DEFAULT_THEME_PALETTE) so the
 * conditions match the original bug screenshots. Captures a screenshot used
 * as the proof bundle artifact.
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import { SNAPSHOTS } from './mock-data';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Proof bundle lives at repo-root/.proofs/fix-1226-1275 (gitignored).
// __dirname is parish/apps/ui/e2e → up four to the repo root.
const PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/fix-1226-1275');

test.describe('chat-feed rendering (#1226, #1275)', () => {
	test.beforeEach(async ({ page }) => {
		// Default light theme — gold player bubble, cream page — matches the bug.
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		// Current location drives the notebook's authored scene while the
		// movement echo remains a separate, fully readable chronicle line.
		await emitEvent(page, 'world-update', {
			...SNAPSHOTS.dusk,
			location_id: 1,
			location_name: 'The Crossroads',
			location_description: 'A quiet crossroads where four narrow roads meet.',
			minute: 59,
		});
	});

	test('#1226 go-to chronicle line shows the full destination', async ({
		page,
	}) => {
		// Backend echoes "> go to <dest>"; the frontend strips "> ".
		await emitEvent(page, 'text-log', {
			id: 'p-goto',
			source: 'player',
			content: '> go to The Crossroads',
		});

		await expect(page.getByLabel('Live chronicle')).toContainText(
			'You: go to The Crossroads',
		);
		await expect(
			page.getByTestId('illustrated-notebook-pixi-host'),
		).toHaveAttribute('data-visible-live-line-keys', /p-goto/);
	});

	test('#1275 multiple NPC reaction chips wrap cleanly under the player chronicle line', async ({
		page,
	}) => {
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

		const strip = page.locator('.notebook-reaction-strip.player');
		const bar = strip.getByTestId('reaction-bar');
		await expect(bar).toBeVisible();

		// AC1275-2: every chip is present, none dropped.
		const badges = strip.locator('.reaction-badge');
		await expect(badges).toHaveCount(reactors.length);

		// AC1275-1: the strip is right-aligned under the player chronicle line.
		await expect(strip).toHaveCSS('justify-content', 'flex-end');
		await expect(strip).toHaveCSS('flex-wrap', 'wrap');
		await expect(bar).toHaveCSS('justify-content', 'flex-end');
		await expect(bar).toHaveCSS('flex-wrap', 'wrap');

		// AC1275-3: each chip is an intact, non-shrinking unit.
		const firstBadge = badges.first();
		await expect(firstBadge).toHaveCSS('white-space', 'nowrap');
		await expect(firstBadge).toHaveCSS('flex-shrink', '0');

		// AC1275-1 (alignment): the chips sit under the right-aligned player
		// chronicle line. The rightmost chip's right edge aligns with the
		// reaction strip, and no chip spills outside the notebook.
		const lastBadge = badges.nth(reactors.length - 1);
		const lastBox = await lastBadge.boundingBox();
		const stripBox = await strip.boundingBox();
		const notebookBox = await page
			.getByTestId('illustrated-notebook-game')
			.boundingBox();
		expect(lastBox).not.toBeNull();
		expect(stripBox).not.toBeNull();
		expect(notebookBox).not.toBeNull();
		if (lastBox && stripBox && notebookBox) {
			// Rightmost chip right edge aligns with the reaction strip's right edge.
			const lastRight = lastBox.x + lastBox.width;
			const stripRight = stripBox.x + stripBox.width;
			expect(Math.abs(lastRight - stripRight)).toBeLessThanOrEqual(4);
			// No chip spills outside the illustrated notebook.
			expect(stripBox.x).toBeGreaterThanOrEqual(notebookBox.x - 1);
			expect(stripRight).toBeLessThanOrEqual(
				notebookBox.x + notebookBox.width + 1,
			);
		}
	});

	test('capture proof screenshot (#1226 + #1275)', async ({ page }) => {
		// Go-to bubble with full destination (#1226).
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

		await expect(page.getByLabel('Live chronicle')).toContainText(
			'You: go to The Crossroads',
		);
		await expect(
			page.locator('.notebook-reaction-strip.player .reaction-badge'),
		).toHaveCount(5);

		await page.screenshot({
			path: path.join(PROOF_DIR, 'chat-feed-rendering.png'),
			fullPage: false,
		});
	});
});
