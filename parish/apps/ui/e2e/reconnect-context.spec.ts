import { expect, test, type WebSocketRoute } from '@playwright/test';
import { installTileRouteMock } from './fixtures';
import { MAP_DATA, NPCS, SNAPSHOTS } from './mock-data';

test('web reconnect commits one epoch aggregate or preserves the whole presentation', async ({
	page,
}) => {
	await installTileRouteMock(page);
	let reconnectState = {
		world: {
			...SNAPSHOTS.morning,
			location_id: 1,
			location_name: 'The Crossroads',
			location_description: 'The old branch opens beside the gate.',
		},
		map: MAP_DATA,
		npcs: NPCS,
		context_epoch: 10,
	};
	let failAggregate = false;
	const sockets: WebSocketRoute[] = [];

	await page.route('**/api/reconnect-state', async (route) => {
		if (failAggregate) {
			await route.fulfill({
				status: 503,
				contentType: 'text/plain',
				body: 'aggregate unavailable',
			});
			return;
		}
		await route.fulfill({ json: reconnectState });
	});
	await page.routeWebSocket('**/api/ws', (socket) => {
		sockets.push(socket);
	});

	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await expect(
		page.locator('[data-testid="illustrated-notebook-game"]'),
	).toBeVisible();
	await expect.poll(() => sockets.length).toBe(1);
	const journalTab = page.getByRole('button', {
		name: 'Open Journal notebook tab',
		exact: true,
	});
	await expect(journalTab).toBeVisible();
	await journalTab.focus();
	await page.keyboard.press('Enter');
	const chronicle = page.getByTestId('notebook-active-section');
	await expect(chronicle).toHaveAttribute('data-section', 'journal');
	const input = page.getByLabel('Player intent', { exact: true });
	await expect(chronicle).toContainText(
		'The old branch opens beside the gate.',
	);

	const sendEvent = async (
		socket: WebSocketRoute,
		event: string,
		payload: unknown,
	) => {
		socket.send(JSON.stringify({ event, payload }));
	};

	// An ordinary reconnect with the same epoch refreshes canonical state but
	// retains the existing transcript and scene-dedup cursor.
	await sendEvent(sockets[0], 'text-log', {
		id: 'same-epoch-line',
		source: 'player',
		content: 'Keep this through an ordinary reconnect.',
	});
	await expect(chronicle).toContainText(
		'Keep this through an ordinary reconnect.',
	);
	await sockets[0].close({ code: 1012, reason: 'ordinary disconnect' });
	await expect.poll(() => sockets.length, { timeout: 10_000 }).toBe(2);
	await expect(chronicle).toContainText(
		'Keep this through an ordinary reconnect.',
	);

	// Start a real UI stream, then make the aggregate fail. No eager
	// StreamManager reset is allowed: partial text and busy state both survive.
	await sendEvent(sockets[1], 'text-log', {
		id: 'half-streamed',
		stream_turn_id: 42,
		source: NPCS[0].real_name,
		content: '',
	});
	await sendEvent(sockets[1], 'stream-token', {
		turn_id: 42,
		message_id: 'half-streamed',
		source: NPCS[0].real_name,
		token: 'I was saying ',
	});
	await expect(chronicle).toContainText('I was saying');
	await expect(input).toHaveAttribute('aria-busy', 'true');

	failAggregate = true;
	await sockets[1].close({ code: 1012, reason: 'aggregate failure' });
	await expect.poll(() => sockets.length, { timeout: 10_000 }).toBe(3);
	await expect(chronicle).toContainText('I was saying');
	await expect(chronicle).toContainText(
		'Keep this through an ordinary reconnect.',
	);
	await expect(input).toHaveAttribute('aria-busy', 'true');

	// Change epoch while disconnected and deliberately omit game-context-reset.
	// The successful aggregate commit must clear all prior presentation state
	// and render fresh prose even though the location name is unchanged.
	failAggregate = false;
	reconnectState = {
		...reconnectState,
		world: {
			...reconnectState.world,
			location_description: 'A fresh branch opens beside the same gate.',
			turn_in_flight: false,
		},
		npcs: [],
		context_epoch: 11,
	};
	await sockets[2].close({ code: 1012, reason: 'missed context reset' });
	await expect.poll(() => sockets.length, { timeout: 10_000 }).toBe(4);
	await expect(chronicle).not.toContainText('I was saying');
	await expect(chronicle).not.toContainText(
		'Keep this through an ordinary reconnect.',
	);
	await expect(chronicle).toContainText(
		'A fresh branch opens beside the same gate.',
	);
	await expect(input).toHaveAttribute('aria-busy', 'false');
});
