import { get } from 'svelte/store';
import type { Readable } from 'svelte/store';
import { demoEnabled, demoPaused, demoTurnCount, demoStatus, demoConfig } from '../stores/demo';
import { streamingActive, textLog, worldState } from '../stores/game';
import { getDemoContext, getLlmPlayerAction, submitInput } from './ipc';

function sleep(ms: number): Promise<void> {
	return new Promise((r) => setTimeout(r, ms));
}

// Wait until `store` is false, with a safety timeout so a missed stream-end
// event never permanently freezes the demo loop.
//
// Uses `let unsub` (not `const`) to avoid the Svelte TDZ bug: Svelte fires
// the subscription callback synchronously with the current value. If the store
// is already false, the callback fires before the `const` assignment completes,
// causing a ReferenceError. A pre-declared `let` avoids this.
function waitForFalse(store: Readable<boolean>, timeoutMs = 30_000): Promise<void> {
	return new Promise((resolve) => {
		let unsub: (() => void) | undefined;
		let timer: ReturnType<typeof setTimeout> | undefined;

		const cleanup = () => {
			unsub?.();
			if (timer !== undefined) clearTimeout(timer);
		};

		// Use a `resolved` flag so the synchronous Svelte subscription case
		// (store already false → callback fires before `unsub` is assigned)
		// is handled correctly: skip cleanup in the callback, then clean up
		// immediately after the subscribe call returns.
		let resolved = false;
		unsub = store.subscribe((v) => {
			if (!v) {
				resolved = true;
				if (unsub) {
					cleanup();
					resolve();
				}
			}
		});

		if (resolved) {
			cleanup();
			resolve();
			return;
		}

		// Safety net: if stream-end never arrives, don't hang forever.
		timer = setTimeout(() => {
			console.warn('demo-player: waitForFalse timed out after', timeoutMs, 'ms');
			cleanup();
			resolve();
		}, timeoutMs);
	});
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

	demoStatus.set('thinking');
	const ctx = await getDemoContext();

	// Fill recent log from frontend text log store (last 40 lines).
	ctx.recent_log = get(textLog)
		.slice(-40)
		.map((e) => `[${e.source}] ${e.content}`);

	// Frontend store's extra_prompt always wins — null means "cleared by user".
	ctx.extra_prompt = config.extra_prompt;

	const action = (await getLlmPlayerAction(ctx)).trim().replace(/^["']|["']$/g, '');

	demoStatus.set('acting');

	// Snapshot streamingActive BEFORE subscribing so we can detect transitions
	// that happen during submitInput. Svelte fires subscriptions synchronously,
	// so we skip the initial value by checking the snapshot instead of a flag.
	const wasActiveAtStart = get(streamingActive);
	let streamingStarted = wasActiveAtStart;

	// Track whether streaming became active during the submit call.
	let unsub: (() => void) | undefined;
	unsub = streamingActive.subscribe((v) => {
		if (v) streamingStarted = true;
	});

	await submitInput(action, []);
	unsub?.();

	if (streamingStarted) {
		// If streaming is already done by the time we reach here, waitForFalse
		// resolves immediately (correct: `let unsub` avoids TDZ).
		await waitForFalse(streamingActive);
	} else {
		// No streaming started (e.g. movement, look, or empty location).
		await sleep(50);
	}

	demoTurnCount.update((n) => n + 1);
	const maxTurns = config.max_turns;
	if (maxTurns != null && get(demoTurnCount) >= maxTurns) {
		demoEnabled.set(false);
		demoStatus.set('idle');
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
