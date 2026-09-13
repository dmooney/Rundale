/**
 * Chat-feed rendering regressions migrated from the notebook Journal.
 */

import { expect, test, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';
import { SNAPSHOTS } from './mock-data';

async function openChat(page: Page) {
	await installTauriMock(page, 'morning');
	await page.goto('/');
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	const chat = page.getByTestId('chat-panel');
	await expect(chat).toBeVisible();
	await emitEvent(page, 'world-update', {
		...SNAPSHOTS.morning,
		location_name: 'The Crossroads',
		location_description: 'A quiet crossroads where four roads meet.',
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
	await expect(page.getByTestId('status-bar')).toContainText('The Crossroads');
	return chat;
}

test.describe('Chat feed rendering', () => {
	test('#1226 a go-to bubble shows the full destination legibly', async ({
		page,
	}) => {
		const chat = await openChat(page);
		await emitEvent(page, 'text-log', {
			id: 'p-goto',
			source: 'player',
			content: '> go to The Crossroads',
		});
		const content = chat.locator('.bubble-row.player .content');
		await expect(content).toHaveText('go to The Crossroads');
		const term = content.locator('.term-location');
		await expect(term).toHaveText('The Crossroads');
		const contrast = await term.evaluate((element) => {
			const termColor = getComputedStyle(element).color;
			const bubble = element.closest('.bubble');
			return {
				termColor,
				bubbleColor: bubble ? getComputedStyle(bubble).backgroundColor : '',
			};
		});
		expect(contrast.termColor).not.toBe(contrast.bubbleColor);
	});

	test('#1275 multiple NPC reaction chips wrap under the player bubble', async ({
		page,
	}) => {
		const chat = await openChat(page);
		await emitEvent(page, 'text-log', {
			id: 'p-react',
			source: 'player',
			content: '> there is not',
		});
		const reactors = [
			['🤔', 'Mick Flanagan'],
			['😏', 'Brendan Duffy'],
			['🙄', 'Padraig Darcy'],
			['😳', 'Fr. Declan Tierney'],
			['👀', 'Roisin Connolly'],
		];
		for (const [emoji, source] of reactors) {
			await emitEvent(page, 'npc-reaction', {
				message_id: 'p-react',
				emoji,
				source,
			});
		}
		const bar = chat.locator('.bubble-row.player [data-testid="reaction-bar"]');
		await expect(bar).toBeVisible();
		await expect(bar).toHaveCSS('justify-content', 'flex-end');
		await expect(bar).toHaveCSS('flex-wrap', 'wrap');
		await expect(bar.locator('.reaction-badge')).toHaveCount(reactors.length);
	});
});
