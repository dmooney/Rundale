import { get } from 'svelte/store';
import type { Readable } from 'svelte/store';
import {
	demoEnabled,
	demoPaused,
	demoTurnCount,
	demoStatus,
	demoConfig,
} from '../stores/demo';
import {
	flushStream,
	streamingActive,
	textLog,
	worldState,
} from '../stores/game';
import { getDemoContext, getLlmPlayerAction, submitInput } from './ipc';

const DEMO_STREAM_WAIT_TIMEOUT_MS = 10_000;

function sleep(ms: number): Promise<void> {
	return new Promise((r) => setTimeout(r, ms));
}

// Wait until `store` is false, with a safety timeout so a missed stream-end
// event never permanently freezes the demo loop.
//
// `unsub` is declared with `const` and `cleanup` as a hoisted function
// declaration so the two can mutually reference each other without TDZ issues.
// `timerHandle` is `let` because it is assigned conditionally (only when the
// store is not already false), so prefer-const does not apply.
function waitForFalse(
	store: Readable<boolean>,
	timeoutMs = DEMO_STREAM_WAIT_TIMEOUT_MS,
	recover?: () => boolean,
): Promise<void> {
	return new Promise((resolve) => {
		// `timerHandle` must be `let` — it is only assigned in the non-resolved
		// branch below and cleanup() must be able to clear it regardless.
		// eslint-disable-next-line prefer-const -- assigned conditionally, not a simple initializer
		let timerHandle: ReturnType<typeof setTimeout> | undefined;

		// Hoisted function declaration so both `const unsub` and `timerHandle`
		// are reachable from cleanup without TDZ issues.
		function cleanup() {
			unsub();
			if (timerHandle !== undefined) clearTimeout(timerHandle);
		}

		// Svelte's `subscribe` runs the callback synchronously once, before
		// `const unsub` is initialized. `started` guards against the callback
		// touching `unsub` during that synchronous run (TDZ): on the sync path
		// we resolve in the callback and release the subscription below, after
		// `unsub` exists; on later (async) runs we can `cleanup()` directly.
		let resolved = false;
		let started = false;
		const unsub = store.subscribe((v) => {
			if (v) return;
			resolved = true;
			resolve();
			if (started) cleanup();
		});
		started = true;

		if (resolved) {
			// Store was already false: callback resolved synchronously above.
			// No timer was scheduled yet; just release the subscription.
			unsub();
			return;
		}

		// Safety net: if stream-end never arrives, don't hang forever.
		timerHandle = setTimeout(() => {
			if (recover?.()) {
				console.info(
					'demo-player: flushed a completed paced stream after',
					timeoutMs,
					'ms',
				);
				cleanup();
				resolve();
				return;
			}
			console.warn(
				'demo-player: waitForFalse timed out after',
				timeoutMs,
				'ms',
			);
			cleanup();
			resolve();
		}, timeoutMs);
	});
}

/** Reveal any completed buffered reply immediately before the autonomous
 * player advances. This is the same safe operation as the player's first
 * keypress: it finalizes every pending bubble and clears streaming state. */
function flushCompletedStream(): boolean {
	get(flushStream)();
	return !get(streamingActive);
}

export async function runDemoTurn(): Promise<void> {
	if (!get(demoEnabled) || get(demoPaused)) return;
	const config = get(demoConfig);

	demoStatus.set('waiting');
	await sleep(config.turn_pause_secs * 1000);
	if (!get(demoEnabled) || get(demoPaused)) return;

	// Unpause the game clock if a keypress or visibility change paused it.
	if (get(worldState)?.paused) {
		await submitInput('/resume', []);
		await sleep(200);
	}

	// #1207 #51: make sure the PREVIOUS turn's NPC reply has fully landed before
	// we snapshot context. A reply can still be revealing (streamingActive) or
	// in backend inference (turn_in_flight) at this point — the inter-turn pause
	// above gives the backend time to begin it. Capturing context now would hand
	// the auto-player a prompt with its own line and no NPC response, which it
	// sometimes continues in the NPC's own voice (role flip). Wait for both to
	// settle. For movement / look / empty-location turns neither is set, so this
	// is a no-op.
	await waitForFalse(
		streamingActive,
		DEMO_STREAM_WAIT_TIMEOUT_MS,
		flushCompletedStream,
	);
	const inflightDeadline = Date.now() + 30_000;
	while (get(worldState)?.turn_in_flight && Date.now() < inflightDeadline) {
		await sleep(100);
	}

	demoStatus.set('thinking');
	const ctx = await getDemoContext();

	// Fill recent log from frontend text log store.
	//
	// #999: drop `[system]` entries that echo the current location description.
	// The static `location_description` field already conveys the scene to the
	// LLM; without this filter, any future or LLM-triggered `look` flood
	// dominates recent_log with copies of the same blurb and the auto-player
	// gets anchored on repeating itself.
	//
	// Filter scene echoes from the FULL textLog BEFORE truncating to 40
	// entries. If we truncated first, a stretch of `look` spam could fill the
	// 40-entry window and leave very little usable history after the filter.
	const sceneHead = ctx.location_description.slice(0, 60).trim();
	const fullLog = get(textLog);
	const filtered = fullLog.filter(
		(e) =>
			!(
				e.source === 'system' &&
				sceneHead.length > 0 &&
				e.content.startsWith(sceneHead)
			),
	);
	ctx.recent_log = filtered.slice(-40).map((e) => `[${e.source}] ${e.content}`);

	// #999: surface the auto-player's own recent actions as a separate field so
	// the demo system prompt can call them out explicitly ("do not repeat your
	// last action"). Take the last 5 `[player]` entries from the FULL log so
	// the anti-repetition signal still works in long runs where the last 5
	// player entries sit outside the 40-entry recent_log window.
	ctx.recent_actions = fullLog
		.filter((e) => e.source === 'player')
		.slice(-5)
		.map((e) => e.content);

	// Frontend store's extra_prompt always wins — null means "cleared by user".
	ctx.extra_prompt = config.extra_prompt;

	const action = (await getLlmPlayerAction(ctx))
		.trim()
		.replace(/^["']|["']$/g, '');

	demoStatus.set('acting');

	// Snapshot streamingActive BEFORE subscribing so we can detect transitions
	// that happen during submitInput. Svelte fires subscriptions synchronously,
	// so we skip the initial value by checking the snapshot instead of a flag.
	const wasActiveAtStart = get(streamingActive);
	let streamingStarted = wasActiveAtStart;

	// Track whether streaming became active during the submit call.
	const unsub = streamingActive.subscribe((v) => {
		if (v) streamingStarted = true;
	});

	await submitInput(action, []);
	unsub?.();

	if (streamingStarted) {
		// If streaming is already done by the time we reach here, waitForFalse
		// resolves immediately (correct: `let unsub` avoids TDZ).
		await waitForFalse(
			streamingActive,
			DEMO_STREAM_WAIT_TIMEOUT_MS,
			flushCompletedStream,
		);
	} else {
		// No streaming started (e.g. movement, look, or empty location).
		await sleep(50);
	}

	demoTurnCount.update((n) => n + 1);
	const maxTurns = config.max_turns;
	if (maxTurns != null && get(demoTurnCount) >= maxTurns) {
		demoEnabled.set(false);
		demoStatus.set('idle');
		// When the demo was launched from the CLI (`--demo-max-turns`),
		// exit the whole process so headless / scripted runs terminate.
		// UI-initiated demos just stop the loop and leave the window open.
		if (config.auto_start) {
			await submitInput('/quit', []);
		}
	}
}

let loopRunning = false;

export async function startDemoLoop(): Promise<void> {
	if (loopRunning) return;
	loopRunning = true;
	demoEnabled.set(true);
	demoTurnCount.set(0);

	while (get(demoEnabled)) {
		if (!get(demoPaused)) {
			try {
				await runDemoTurn();
			} catch (e) {
				console.warn('Demo turn error:', e);
				await sleep(2000);
			}
		} else {
			await sleep(500);
		}
	}

	loopRunning = false;
	demoStatus.set('idle');
}

export function stopDemo(): void {
	demoEnabled.set(false);
}
