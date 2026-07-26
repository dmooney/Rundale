/**
 * E2E tests for user interactions: input submission, streaming, paused state.
 */

import { test, expect, installTauriMock, emitEvent } from './fixtures';
import { SNAPSHOTS, IRISH_HINTS } from './mock-data';

test.describe('Input field interactions', () => {
	test.beforeEach(async ({ page }) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	test('can type and submit text via Enter key', async ({ page }) => {
		const input = page.getByLabel('Player intent');
		await input.fill('go to Howth');
		await input.press('Enter');

		// Input should be cleared after submission
		await expect(input).toHaveValue('');
	});

	// #1379: the native notebook input remains physically editable so the
	// first keystroke can flush an in-flight reply. aria-busy communicates
	// the busy state without using the HTML disabled attribute.
	test('input stays editable during streaming (flush-on-interaction, #1379)', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent');

		// Simulate loading/streaming state
		await emitEvent(page, 'loading', { active: true });

		await expect(input).toBeEditable();
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(page.getByLabel('Live chronicle · listening')).toBeAttached();
		await input.press('x');
		await expect(input).toHaveValue('x');

		// End loading
		await emitEvent(page, 'loading', { active: false });
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(page.getByLabel('Live chronicle')).toBeAttached();
	});

	// Regression for #991: the backend's handle_npc_conversation cancels
	// and re-spawns the loading animation per addressed NPC turn, so
	// `loading {active:false}` arrives mid-chain (between phase-1 NPC
	// turns, or between phase-1 and the autonomous follow-up chain).
	// The mid-chain loading=false must NOT clear the notebook's busy state —
	// only the terminal `stream-end` may.
	test('input stays in streaming state across mid-chain loading=false (#991)', async ({
		page,
	}) => {
		const input = page.getByLabel('Player intent');

		// Chain begins.
		await emitEvent(page, 'loading', { active: true });
		await expect(input).toBeEditable();
		await expect(input).toHaveAttribute('aria-busy', 'true');

		// NPC 1 streams a reply and the per-turn cancel fires.
		await emitEvent(page, 'stream-token', {
			token: 'Dia dhuit. ',
			turn_id: 1001,
			source: 'Padraig',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1001 });
		await emitEvent(page, 'loading', { active: false });

		// Input must remain in streaming state even though loading=false has
		// arrived, because the chain has not yet emitted `stream-end`.
		await expect(input).toHaveAttribute('aria-busy', 'true');
		await expect(page.getByLabel('Live chronicle · listening')).toBeAttached();

		// Capture the mid-chain state as proof for #991 (rule #10 screenshot tier).
		await page.screenshot({
			path: '../../../docs/proofs/991-streaming-active-chain-gap/screenshots/mid-chain-input-streaming.png',
			fullPage: false,
		});

		// Autonomous follow-up turn (no fresh loading=true in this path).
		await emitEvent(page, 'stream-token', {
			token: 'Aye, indeed.',
			turn_id: 1002,
			source: 'Siobhan',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1002 });

		// Still streaming — chain still alive.
		await expect(input).toHaveAttribute('aria-busy', 'true');

		// Chain terminates.
		await emitEvent(page, 'stream-end', { hints: [] });

		// Only now does the busy state clear and the field return to idle.
		await expect(input).toHaveAttribute('aria-busy', 'false');
		await expect(page.getByLabel('Live chronicle')).toBeAttached();
	});
});

test.describe('Streaming simulation', () => {
	test('stream tokens appear incrementally in the live chronicle', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		// Start loading
		await emitEvent(page, 'loading', { active: true });

		// Send tokens
		await emitEvent(page, 'stream-token', {
			token: 'Ah, ',
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-token', {
			token: "you're ",
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-token', {
			token: 'welcome!',
			turn_id: 1,
			source: 'Siobhan Murphy',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 1 });

		await expect(page.getByLabel(/Live chronicle/)).toContainText(
			"Ah, you're welcome!",
		);

		// End stream
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });
	});

	test('keeps overlapping multi-npc streams attached to the right speaker', async ({
		page,
	}) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await emitEvent(page, 'loading', { active: true });

		await emitEvent(page, 'text-log', {
			id: 'msg-1',
			source: 'Séamas Ó Briain',
			content: '',
			stream_turn_id: 11,
		});
		await emitEvent(page, 'stream-token', {
			token: 'I heard the fair will be lively tonight ',
			turn_id: 11,
			source: 'Séamas Ó Briain',
		});
		await expect(page.getByLabel(/Live chronicle/)).toContainText(
			'Séamas Ó Briain: I heard the fair will be lively tonight',
		);

		// Queue Aoife before Séamas has finished animating.
		await emitEvent(page, 'text-log', {
			id: 'msg-2',
			source: 'Aoife Ní Cheallaigh',
			content: '',
			stream_turn_id: 12,
		});
		await emitEvent(page, 'stream-token', {
			token: "If it is, I'll bring the cart before sunset.",
			turn_id: 12,
			source: 'Aoife Ní Cheallaigh',
		});

		await emitEvent(page, 'stream-token', {
			token: 'with music by the square.',
			turn_id: 11,
			source: 'Séamas Ó Briain',
		});
		await emitEvent(page, 'stream-turn-end', { turn_id: 11 });
		await emitEvent(page, 'stream-turn-end', { turn_id: 12 });
		await emitEvent(page, 'stream-end', { hints: IRISH_HINTS });

		const chronicle = page.getByLabel('Live chronicle');
		await expect(chronicle).toContainText(
			'Séamas Ó Briain: I heard the fair will be lively tonight with music by the square.',
		);
		await expect(chronicle).toContainText(
			"Aoife Ní Cheallaigh: If it is, I'll bring the cart before sunset.",
		);
	});
});

test.describe('Paused state', () => {
	test('resumes a paused game before submitting a player action', async ({
		page,
	}) => {
		const pausedSnapshot = { ...SNAPSHOTS.morning, paused: true };
		await installTauriMock(page, 'morning');

		// Override the snapshot with paused state
		await page.addInitScript(
			({ snapshot }) => {
				const responses = (
					window as unknown as Record<string, Record<string, unknown>>
				).__TEST_MOCK_RESPONSES__;
				if (responses) {
					responses['get_world_snapshot'] = snapshot;
					responses['get_reconnect_state'] = {
						...(responses['get_reconnect_state'] as Record<string, unknown>),
						world: snapshot,
					};
				}
			},
			{ snapshot: pausedSnapshot },
		);

		await page.goto('/');
		await page.waitForLoadState('networkidle');

		await page.evaluate(() => {
			const target = window as unknown as Record<string, unknown>;
			const tauri = target.__TAURI_INTERNALS__ as {
				invoke: (
					command: string,
					args?: Record<string, unknown>,
				) => Promise<unknown>;
			};
			const originalInvoke = tauri.invoke.bind(tauri);
			const submitted: string[] = [];
			target.__TEST_SUBMITTED_INPUTS__ = submitted;
			tauri.invoke = async (
				command: string,
				args?: Record<string, unknown>,
			) => {
				if (command === 'submit_input' && typeof args?.text === 'string') {
					submitted.push(args.text);
				}
				return originalInvoke(command, args);
			};
		});

		const input = page.getByLabel('Player intent');
		await input.fill('look around the yard');
		await input.press('Enter');
		await expect
			.poll(() =>
				page.evaluate(
					() =>
						(window as unknown as Record<string, string[]>)
							.__TEST_SUBMITTED_INPUTS__,
				),
			)
			.toEqual(['/resume', 'look around the yard']);
	});
});

test.describe('Festival badge', () => {
	test('shows an active festival in notebook time details', async ({
		page,
	}) => {
		const festivalSnapshot = { ...SNAPSHOTS.morning, festival: 'Samhain' };
		await installTauriMock(page, 'morning');

		await page.addInitScript(
			({ snapshot }) => {
				const responses = (
					window as unknown as Record<string, Record<string, unknown>>
				).__TEST_MOCK_RESPONSES__;
				if (responses) {
					responses['get_world_snapshot'] = snapshot;
					responses['get_reconnect_state'] = {
						...(responses['get_reconnect_state'] as Record<string, unknown>),
						world: snapshot,
					};
				}
			},
			{ snapshot: festivalSnapshot },
		);

		await page.goto('/');
		await page.waitForLoadState('networkidle');

		const timeControl = page.getByRole('button', {
			name: 'Open time details',
		});
		await timeControl.focus();
		await page.keyboard.press('Enter');
		await expect(page.getByLabel('time drawer')).toContainText('Samhain');
	});
});
