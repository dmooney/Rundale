/**
 * E2E proof for the chat-feed rendering fixes:
 *
 *  - #1226: a go-to / movement player echo bubble must show the FULL
 *    destination. The destination equals the current location name, so it is
 *    highlighted as a `.term-location`. On the gold player bubble that term
 *    colour (#b58900) was gold-on-gold and the destination vanished. The fix
 *    forces player-bubble term spans to the bubble's readable foreground.
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
import type { Page } from '@playwright/test';
import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// Proof bundle lives at repo-root/.proofs/fix-1226-1275 (gitignored).
// __dirname is parish/apps/ui/e2e → up four to the repo root.
const PROOF_DIR = path.resolve(__dirname, '../../../../.proofs/fix-1226-1275');
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

test.describe('chat-feed rendering (#1226, #1275)', () => {
	test.beforeEach(async ({ page }) => {
		// Default light theme — gold player bubble, cream page — matches the bug.
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await openJournal(page);
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

	test('#1226 go-to bubble shows the full destination', async ({ page }) => {
		const chatPanel = journalOverlay(page).getByTestId('chat-panel');

		// Backend echoes "> go to <dest>"; the frontend strips "> ".
		await emitEvent(page, 'text-log', {
			id: 'p-goto',
			source: 'player',
			content: '> go to The Crossroads',
		});

		const playerContent = chatPanel.locator('.bubble-row.player .content');
		await expect(playerContent).toHaveText('go to The Crossroads');

		// The destination is highlighted as a location term…
		const term = chatPanel.locator('.bubble-row.player .term-location');
		await expect(term).toHaveText('The Crossroads');

		// …and that term is legible: its colour must NOT be the page-tuned gold
		// location colour (which equals the bubble background). The fix sets it
		// to the containing notebook sheet's --color-bg.
		const termColor = await term.evaluate((el) => getComputedStyle(el).color);
		const sheetColor = await journalOverlay(page).evaluate((sheet) => {
			const probe = document.createElement('span');
			probe.style.color = 'var(--color-bg)';
			sheet.appendChild(probe);
			const color = getComputedStyle(probe).color;
			probe.remove();
			return color;
		});
		expect(termColor).toBe(sheetColor);
		expect(termColor).not.toBe('rgb(181, 137, 0)');
	});

	test('#1275 multiple NPC reaction chips wrap cleanly and align under the bubble', async ({
		page,
	}) => {
		const chatPanel = journalOverlay(page).getByTestId('chat-panel');

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

		const bar = chatPanel.locator(
			'.bubble-row.player [data-testid="reaction-bar"]',
		);
		await expect(bar).toBeVisible();

		// AC1275-2: every chip is present, none dropped.
		const badges = chatPanel.locator('.bubble-row.player .reaction-badge');
		await expect(badges).toHaveCount(reactors.length);

		// AC1275-1: the bar is right-aligned (under the right-aligned bubble).
		await expect(bar).toHaveCSS('justify-content', 'flex-end');
		await expect(bar).toHaveCSS('flex-wrap', 'wrap');

		// AC1275-3: each chip is an intact, non-shrinking unit.
		const firstBadge = badges.first();
		await expect(firstBadge).toHaveCSS('white-space', 'nowrap');
		await expect(firstBadge).toHaveCSS('flex-shrink', '0');

		// AC1275-1 (alignment): the chips sit under the right-aligned player
		// bubble. The rightmost chip's right edge aligns with the bubble's right
		// edge, and no chip spills off the left of the chat panel.
		const bubble = chatPanel.locator('.bubble-row.player .bubble').first();
		const lastBadge = badges.nth(reactors.length - 1);
		const lastBox = await lastBadge.boundingBox();
		const bubbleBox = await bubble.boundingBox();
		const barBox = await bar.boundingBox();
		const panelBox = await chatPanel.boundingBox();
		expect(lastBox).not.toBeNull();
		expect(bubbleBox).not.toBeNull();
		expect(barBox).not.toBeNull();
		expect(panelBox).not.toBeNull();
		if (lastBox && bubbleBox && barBox && panelBox) {
			// Rightmost chip right edge aligns with the bubble's right edge.
			const lastRight = lastBox.x + lastBox.width;
			const bubbleRight = bubbleBox.x + bubbleBox.width;
			expect(Math.abs(lastRight - bubbleRight)).toBeLessThanOrEqual(4);
			// No chip spills off the left edge of the chat panel.
			expect(barBox.x).toBeGreaterThanOrEqual(panelBox.x - 1);
		}
	});

	test('capture proof screenshot (#1226 + #1275)', async ({ page }) => {
		const journal = journalOverlay(page);
		const chatPanel = journal.getByTestId('chat-panel');

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

		await expect(
			chatPanel.locator('.bubble-row.player .content').first(),
		).toHaveText('go to The Crossroads');
		await expect(
			chatPanel.locator('.bubble-row.player .reaction-badge'),
		).toHaveCount(5);
		await expect(journal).toHaveAttribute('data-surface', 'journal');

		// Keep the full viewport so the artifact proves the legacy chat feed is
		// contained by the illustrated notebook's Journal sheet.
		fs.mkdirSync(PROOF_DIR, { recursive: true });
		await page.screenshot({
			path: path.join(PROOF_DIR, 'chat-feed-rendering.png'),
			fullPage: false,
		});
	});
});
