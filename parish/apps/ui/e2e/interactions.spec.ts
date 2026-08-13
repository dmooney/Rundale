/**
 * Player-input, streaming, and state contracts on the chat shell.
 */

import { expect, test, installTauriMock, emitEvent } from './fixtures';
import type { Page } from '@playwright/test';
import { IRISH_HINTS, SNAPSHOTS } from './mock-data';
import type { NpcInfo } from '../src/lib/types';

const STREAM_NPCS: NpcInfo[] = [
	{
		name: 'Siobhan Murphy',
		real_name: 'Siobhan Murphy',
		occupation: 'Farmer',
		mood: 'determined',
		introduced: true,
		mood_emoji: '•',
	},
	{
		name: 'Padraig Darcy',
		real_name: 'Padraig Darcy',
		occupation: 'Publican',
		mood: 'content',
		introduced: true,
		mood_emoji: '•',
	},
];

async function waitForChat(page: Page) {
	await expect(page.getByTestId('app-root')).toHaveAttribute(
		'data-controller-ready',
		'true',
	);
	await expect(
		page.getByRole('combobox', { name: 'Player input' }),
	).toBeEditable();
}

async function submissions(page: Page): Promise<string[]> {
	return page.evaluate(() =>
		(
			window as unknown as {
				__TEST_INVOKE_CALLS__: Array<{
					command: string;
					args?: { text?: string };
				}>;
			}
		).__TEST_INVOKE_CALLS__
			.filter((call) => call.command === 'submit_input')
			.map((call) => String(call.args?.text ?? '')),
	);
}

async function seedOverflowingTranscript(page: Page) {
	for (let index = 0; index < 18; index += 1) {
		await emitEvent(page, 'text-log', {
			id: `history-${index}`,
			source: 'system',
			content: `Earlier parish event ${index + 1}: the road, weather, and neighbours remain in view.`,
		});
	}
}

async function streamReply(page: Page, turnId: number, reply: string) {
	await emitEvent(page, 'loading', { active: true });
	await emitEvent(page, 'text-log', {
		id: `reply-${turnId}`,
		source: 'Siobhan Murphy',
		content: '',
		stream_turn_id: turnId,
	});
	await emitEvent(page, 'stream-token', {
		token: reply,
		turn_id: turnId,
		source: 'Siobhan Murphy',
		message_id: `reply-${turnId}`,
	});
	await emitEvent(page, 'stream-turn-end', { turn_id: turnId });
	await emitEvent(page, 'stream-end', { hints: [] });
	await expect(page.getByTestId('input-field')).toHaveAttribute(
		'aria-busy',
		'false',
		{ timeout: 15_000 },
	);
}

async function expectLatestReplyVisible(page: Page, reply: string) {
	const chat = page.getByTestId('chat-panel');
	const row = chat.locator('.bubble-row.npc').filter({ hasText: reply }).last();
	await expect(row).toBeVisible();
	await expect
		.poll(() =>
			chat.evaluate(
				(element) =>
					element.scrollHeight - element.scrollTop - element.clientHeight,
			),
		)
		.toBeLessThanOrEqual(1);
	const geometry = await page.evaluate(() => {
		const chatElement = document.querySelector<HTMLElement>(
			'[data-testid="chat-panel"]',
		)!;
		const replyElement = [
			...document.querySelectorAll<HTMLElement>('.bubble-row.npc'),
		].at(-1)!;
		const inputElement = document.querySelector<HTMLElement>('.input-wrapper')!;
		const chatRect = chatElement.getBoundingClientRect();
		const replyRect = replyElement.getBoundingClientRect();
		const inputRect = inputElement.getBoundingClientRect();
		return {
			chatTop: chatRect.top,
			chatBottom: chatRect.bottom,
			replyTop: replyRect.top,
			replyBottom: replyRect.bottom,
			inputTop: inputRect.top,
			bottomDelta:
				chatElement.scrollHeight -
				chatElement.scrollTop -
				chatElement.clientHeight,
		};
	});
	expect(geometry.replyTop).toBeGreaterThanOrEqual(geometry.chatTop - 1);
	expect(geometry.replyBottom).toBeLessThanOrEqual(geometry.chatBottom + 1);
	expect(geometry.replyBottom).toBeLessThanOrEqual(geometry.inputTop + 1);
	expect(Math.abs(geometry.bottomDelta)).toBeLessThanOrEqual(1);
}

test.describe('Input and streaming', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await waitForChat(page);
	});

	test('can type and submit text via Enter', async ({ page }) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await input.fill('go to Howth');
		await input.press('Enter');
		await expect(input).toHaveText('');
		await expect.poll(() => submissions(page)).toEqual(['go to Howth']);
	});

	test('first interaction flushes an in-flight reply and remains editable', async ({
		page,
	}) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'stream-token', {
			token: 'The whole reply appears when the player starts typing again.',
			turn_id: 1379,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1379 });
		await expect(input).toHaveAttribute('aria-busy', 'true');

		await input.press('x');
		await expect(input).toHaveText('x');
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(page.getByTestId('chat-panel')).toContainText(
			'The whole reply appears when the player starts typing again.',
		);
	});

	test('mid-chain loading=false does not clear the streaming state', async ({
		page,
	}) => {
		const input = page.getByRole('combobox', { name: 'Player input' });
		await emitEvent(page, 'loading', { active: true });
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await emitEvent(page, 'stream-token', {
			token: 'Dia dhuit.',
			turn_id: 1001,
			source: 'Padraig',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1001 });
		await emitEvent(page, 'loading', { active: false });
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await emitEvent(page, 'stream-end', { hints: [] });
		await expect(input).toHaveAttribute('aria-busy', 'false');
	});

	test('stream tokens appear incrementally in the visible transcript', async ({
		page,
	}) => {
		await emitEvent(page, 'loading', { active: true });
		for (const token of ['Ah, ', "you're ", 'welcome!']) {
			await emitEvent(page, 'stream-token', {
				token,
				turn_id: 1,
				source: 'Siobhan Murphy',
			});
		}
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });
		await expect(page.getByTestId('chat-panel')).toContainText(
			"Ah, you're welcome!",
		);
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
	});

	test('only post-validation dialogue can become visible', async ({ page }) => {
		const forbiddenCandidate =
			'The agricultural show committee has very strong opinions.';
		const safeFallback = 'I beg your pardon; I lost the thread of that.';
		const chat = page.getByTestId('chat-panel');

		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'text-log', {
			id: 'validated-turn',
			source: 'Siobhan Murphy',
			content: '',
			stream_turn_id: 1834,
		});
		// Even if internal telemetry is forwarded by a test transport, it is not
		// part of the renderable UI protocol and has no store listener.
		await emitEvent(page, 'dialogue-candidate-token', {
			token: forbiddenCandidate,
			turn_id: 1834,
			source: 'provider',
		});
		await expect(chat).not.toContainText(forbiddenCandidate);

		// The backend has quarantined the candidate and exposes only the
		// canonical validator outcome through the renderable protocol.
		await emitEvent(page, 'stream-token', {
			token: safeFallback,
			turn_id: 1834,
			source: 'Siobhan Murphy',
			message_id: 'validated-turn',
		});
		await expect(chat).not.toContainText(forbiddenCandidate);

		await emitEvent(page, 'stream-turn-end', { turn_id: 1834 });
		await expect(chat).toContainText(safeFallback);
		await emitEvent(page, 'stream-end', { hints: [] });
		await expect(chat).not.toContainText(forbiddenCandidate);
	});

	test('typed grounding contradictions never transiently render', async ({
		page,
	}) => {
		const rejectedCandidates = [
			'There is no old bridge in Kilteevan that I have ever heard tell of.',
			'A small mark like that turns a plain scrap of silk into a whole life\'s remembrance.',
			'Sunday is market day in the town.',
			'There is no one singer taking the floor; only the general clatter of the room.',
		];
		const safeFallback = 'I beg your pardon; I lost the thread of that.';
		const chat = page.getByTestId('chat-panel');

		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'text-log', {
			id: 'typed-grounding-turn',
			source: 'Padraig Darcy',
			content: '',
			stream_turn_id: 1872,
		});
		for (const candidate of rejectedCandidates) {
			await emitEvent(page, 'dialogue-candidate-token', {
				token: candidate,
				turn_id: 1872,
				source: 'provider',
			});
			await expect(chat).not.toContainText(candidate);
		}
		await emitEvent(page, 'stream-token', {
			token: safeFallback,
			turn_id: 1872,
			source: 'Padraig Darcy',
			message_id: 'typed-grounding-turn',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1872 });
		await emitEvent(page, 'stream-end', { hints: [] });
		await expect(chat).toContainText(safeFallback);
		for (const candidate of rejectedCandidates) {
			await expect(chat).not.toContainText(candidate);
		}
	});

	test('an incomplete multi-facet candidate never appears before its complete fallback', async ({
		page,
	}) => {
		const incompleteCandidate =
			"'Tis a fine morning indeed. What brings ye to this church?";
		const completeFallback =
			'I hear that Peig Hannigan sent you. Aiden Carney, is it? I have it. I cannot promise work, but I understand you are seeking it. I cannot promise lodging, but I understand you need a dry place to sleep.';
		const chat = page.getByTestId('chat-panel');

		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'text-log', {
			id: 'obligation-turn',
			source: 'Fr. Declan Tierney',
			content: '',
			stream_turn_id: 1832,
		});
		await emitEvent(page, 'dialogue-candidate-token', {
			token: incompleteCandidate,
			turn_id: 1832,
			source: 'provider',
		});
		await expect(chat).not.toContainText(incompleteCandidate);

		await emitEvent(page, 'stream-token', {
			token: completeFallback,
			turn_id: 1832,
			source: 'Fr. Declan Tierney',
			message_id: 'obligation-turn',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1832 });
		await expect(chat).toContainText('Peig Hannigan');
		await expect(chat).toContainText('Aiden Carney');
		await expect(chat).toContainText('cannot promise work');
		await expect(chat).toContainText('cannot promise lodging');
		await expect(chat).not.toContainText(incompleteCandidate);
		await emitEvent(page, 'stream-end', { hints: [] });
		await expect(chat).not.toContainText(incompleteCandidate);
	});

	test('overlapping NPC turns remain attached to their speakers', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning', { npcs: STREAM_NPCS });
		await page.goto('/');
		await waitForChat(page);
		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'text-log', {
			id: 'msg-1',
			source: 'Siobhan Murphy',
			content: '',
			stream_turn_id: 11,
		});
		await emitEvent(page, 'stream-token', {
			token: 'I heard the fair will be lively tonight ',
			turn_id: 11,
			source: 'Siobhan Murphy',
		});
		const chat = page.getByTestId('chat-panel');
		const siobhanRow = chat
			.locator('.bubble-row.npc')
			.filter({ hasText: 'Siobhan Murphy' });
		await expect(siobhanRow).toContainText(
			'I heard the fair will be lively tonight',
		);

		await emitEvent(page, 'text-log', {
			id: 'msg-2',
			source: 'Padraig Darcy',
			content: '',
			stream_turn_id: 12,
		});
		await emitEvent(page, 'stream-token', {
			token: "If it is, I'll bring the cart before sunset.",
			turn_id: 12,
			source: 'Padraig Darcy',
		});
		await emitEvent(page, 'stream-token', {
			token: 'with music by the square.',
			turn_id: 11,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 11 });
		await emitEvent(page, 'stream-turn-end', { turn_id: 12 });
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
		const padraigRow = chat
			.locator('.bubble-row.npc')
			.filter({ hasText: 'Padraig Darcy' });
		await expect(siobhanRow.locator('.label')).toHaveText('Siobhan Murphy');
		await expect(siobhanRow).toContainText(
			'I heard the fair will be lively tonight with music by the square.',
		);
		await expect(padraigRow.locator('.label')).toHaveText('Padraig Darcy');
		await expect(padraigRow).toContainText(
			"If it is, I'll bring the cart before sunset.",
		);
	});

	for (const viewport of [
		{ name: 'desktop', width: 1280, height: 800 },
		{ name: 'mobile', width: 390, height: 844 },
	]) {
		test(`${viewport.name} keeps a completed streamed reply above the composer`, async ({
			page,
		}) => {
			await page.setViewportSize(viewport);
			await seedOverflowingTranscript(page);
			const reply =
				'The potatoes need tending first, then we will mend the western gate before the evening rain.';
			await streamReply(page, viewport.width, reply);
			await expectLatestReplyVisible(page, reply);
		});
	}

	test('mobile preserves a reader who scrolls up during a streamed reply', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await seedOverflowingTranscript(page);
		await emitEvent(page, 'loading', { active: true });
		await emitEvent(page, 'stream-token', {
			token:
				'First, ye will clear the stones from the field. After that, bring the seed from the dry loft and mind the broken stair.',
			turn_id: 1835,
			source: 'Siobhan Murphy',
		});
		await expect(page.getByTestId('chat-panel')).toContainText('First,');
		const chat = page.getByTestId('chat-panel');
		await chat.hover();
		await page.mouse.wheel(0, -2_000);
		await expect
			.poll(() => chat.evaluate((element) => element.scrollTop))
			.toBe(0);
		const topAfterUserScroll = await chat.evaluate(
			(element) => element.scrollTop,
		);

		await emitEvent(page, 'stream-turn-end', { turn_id: 1835 });
		await emitEvent(page, 'stream-end', { hints: [] });
		await expect(page.getByTestId('input-field')).toHaveAttribute(
			'aria-busy',
			'false',
			{ timeout: 15_000 },
		);
		expect(await chat.evaluate((element) => element.scrollTop)).toBe(
			topAfterUserScroll,
		);
	});

	test('mobile bottom-follow survives viewport and composer height changes', async ({
		page,
	}) => {
		await page.setViewportSize({ width: 390, height: 844 });
		await seedOverflowingTranscript(page);
		const reply = 'The western gate is ready for us now.';
		await streamReply(page, 1836, reply);
		await expectLatestReplyVisible(page, reply);

		await page.setViewportSize({ width: 390, height: 700 });
		await expectLatestReplyVisible(page, reply);

		await page
			.getByTestId('input-field')
			.fill('First line\nSecond line\nThird line\nFourth line');
		await expectLatestReplyVisible(page, reply);
	});
});

test.describe('World-state chrome', () => {
	test('shows a paused indicator', async ({ page }) => {
		await installTauriMock(page, 'morning', {
			snapshot: { ...SNAPSHOTS.morning, paused: true },
		});
		await page.goto('/');
		await waitForChat(page);
		await expect(page.getByTestId('status-bar')).toContainText('Paused');
	});

	test('does not label the clock paused for inference-only pauses', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning', {
			snapshot: { ...SNAPSHOTS.morning, inference_paused: true },
		});
		await page.goto('/');
		await waitForChat(page);
		await expect(page.getByTestId('status-bar')).not.toContainText('Paused');
	});

	test('shows an active festival', async ({ page }) => {
		await installTauriMock(page, 'morning', {
			snapshot: { ...SNAPSHOTS.morning, festival: 'Samhain' },
		});
		await page.goto('/');
		await waitForChat(page);
		await expect(page.getByTestId('status-bar')).toContainText('Samhain');
	});
});
