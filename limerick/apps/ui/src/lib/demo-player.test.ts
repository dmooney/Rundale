import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
	demoEnabled,
	demoPaused,
	demoStatus,
	demoTurnCount,
	demoConfig,
} from '../stores/demo';
import { flushStream, streamingActive, textLog } from '../stores/game';
import { runDemoTurn, stopDemo } from './demo-player';
import { getDemoContext, getLlmPlayerAction, submitInput } from './ipc';
import { createStreamManager } from './setup/stream-manager';

const testConfig = {
	auto_start: false,
	extra_prompt: null,
	turn_pause_secs: 0.01,
	max_turns: null,
};

const KILTEEVAN_SCENE =
	'The small village of Kilteevan — a handful of whitewashed cottages clustered around a well.';

vi.mock('../lib/ipc', () => ({
	getDemoContext: vi.fn(async () => ({
		location_name: 'Kilteevan',
		location_description: KILTEEVAN_SCENE,
		game_time: 'Wednesday, 14 May 1820, morning',
		season: 'spring',
		weather: 'clear sky',
		npcs_here: [],
		adjacent: [],
		active_tasks: [],
		recent_log: [],
		recent_actions: [],
		extra_prompt: null,
	})),
	getLlmPlayerAction: vi.fn(async () => '"Good morning"'),
	submitInput: vi.fn(async () => {}),
}));

beforeEach(() => {
	demoEnabled.set(false);
	demoPaused.set(false);
	demoStatus.set('idle');
	demoTurnCount.set(0);
	demoConfig.set(testConfig);
	textLog.set([]);
	streamingActive.set(false);
	flushStream.set(() => 0);
	vi.mocked(submitInput).mockClear();
	vi.mocked(getLlmPlayerAction).mockClear();
	vi.mocked(getDemoContext).mockClear();
});

describe('stopDemo', () => {
	it('sets demoEnabled to false', () => {
		demoEnabled.set(true);
		stopDemo();
		expect(get(demoEnabled)).toBe(false);
	});
});

describe('runDemoTurn', () => {
	it('returns immediately when demo is disabled', async () => {
		demoEnabled.set(false);
		await expect(runDemoTurn()).resolves.toBeUndefined();
	});

	it('returns immediately when demo is paused', async () => {
		demoEnabled.set(true);
		demoPaused.set(true);
		await expect(runDemoTurn()).resolves.toBeUndefined();
	});

	it('sets status to waiting when demo is active', async () => {
		demoEnabled.set(true);
		// runDemoTurn will set status to 'waiting', then sleep.
		// We use a timeout to prevent the test from hanging on the sleep.
		const promise = runDemoTurn();
		expect(get(demoStatus)).toBe('waiting');
		// Let the turn complete (sleep timeout)
		await promise;
	});

	it('dispatches /quit when CLI demo reaches max_turns', async () => {
		demoEnabled.set(true);
		demoTurnCount.set(2);
		demoConfig.set({ ...testConfig, auto_start: true, max_turns: 3 });

		await runDemoTurn();

		expect(get(demoEnabled)).toBe(false);
		expect(get(demoStatus)).toBe('idle');
		expect(submitInput).toHaveBeenCalledWith('/quit', []);
	});

	it('does not dispatch /quit for UI-launched demos at max_turns', async () => {
		demoEnabled.set(true);
		demoTurnCount.set(2);
		demoConfig.set({ ...testConfig, auto_start: false, max_turns: 3 });

		await runDemoTurn();

		expect(get(demoEnabled)).toBe(false);
		expect(submitInput).not.toHaveBeenCalledWith('/quit', []);
	});

	// Regression for #991: simulate the +page.svelte onLoading handler
	// in tandem with stream-manager. A conversation chain emits
	// loading(true) → stream-token → loading(false) → (autonomous chain
	// tokens) → stream-end. With the chainInProgress gate in place,
	// streamingActive must stay true across the mid-chain loading=false
	// so the demo's waitForFalse(streamingActive) does NOT resolve until
	// the chain's stream-end fires.
	it('waits_through_per_turn_loading_false_within_chain', async () => {
		const sm = createStreamManager();

		// Inline replica of the +page.svelte onLoading gate.
		const handleLoading = (active: boolean) => {
			if (active) {
				streamingActive.set(true);
			} else if (
				!sm.isChainInProgress() &&
				sm.pendingTurnCount() === 0 &&
				!sm.hasPendingEndHints()
			) {
				streamingActive.set(false);
			}
		};

		demoEnabled.set(true);
		demoConfig.set({ ...testConfig, turn_pause_secs: 0 });

		// submitInput drives the event sequence the bug describes.
		vi.mocked(submitInput).mockImplementationOnce(async () => {
			// loading=true → chain starts at the UI level.
			handleLoading(true);
			expect(get(streamingActive)).toBe(true);

			// NPC 1 streams a token + completes.
			const t1 = sm.queuePendingTurn(1, 'Padraig');
			t1.buffer = 'Hello';
			t1.complete = true;
			sm.startTurnPumpIfNeeded(t1);
			sm.finalizePendingTurn(1);

			// Mid-chain: per-turn cancel emits loading=false. The gate
			// MUST keep streamingActive true because the chain is mid-flight.
			handleLoading(false);
			expect(get(streamingActive)).toBe(true);
			expect(sm.isChainInProgress()).toBe(true);

			// Autonomous follow-up turn (no fresh loading=true is emitted by
			// the backend autonomous path).
			const t2 = sm.queuePendingTurn(2, 'Siobhan');
			t2.buffer = 'Aye.';
			t2.complete = true;
			sm.startTurnPumpIfNeeded(t2);
			sm.finalizePendingTurn(2);

			// Chain terminates only on stream-end.
			sm.setPendingEndHints([]);
			sm.maybeFinishNpcStream();
		});

		await runDemoTurn();

		// After the chain ends, streamingActive must be false and the
		// chain flag reset — proving the demo loop waited for the full
		// chain rather than resolving on the mid-chain loading=false.
		expect(get(streamingActive)).toBe(false);
		expect(sm.isChainInProgress()).toBe(false);
	});

	// Regression for #1207 #51: a previous turn's NPC reply can still be
	// revealing (streamingActive) when the next turn begins. The loop must wait
	// for it to settle BEFORE snapshotting context, or the auto-player gets a
	// prompt missing the NPC line and sometimes answers in the NPC's own voice.
	it('waits for an in-flight NPC reply to settle before snapshotting context', async () => {
		demoEnabled.set(true);
		// Simulate the prior turn's reply still revealing as this turn starts.
		streamingActive.set(true);

		let streamingAtContext: boolean | null = null;
		vi.mocked(getDemoContext).mockImplementationOnce(async () => {
			streamingAtContext = get(streamingActive);
			return {
				location_name: 'Kilteevan',
				location_description: KILTEEVAN_SCENE,
				game_time: 'Wednesday, 14 May 1820, morning',
				season: 'spring',
				weather: 'clear sky',
				npcs_here: [],
				adjacent: [],
				active_tasks: [],
				recent_log: [],
				recent_actions: [],
				extra_prompt: null,
			};
		});

		// The reveal finishes shortly after the turn begins.
		setTimeout(() => streamingActive.set(false), 80);

		await runDemoTurn();

		expect(streamingAtContext).toBe(false);
		expect(getLlmPlayerAction).toHaveBeenCalledTimes(1);
	});

	it('flushes a completed paced stream at the bounded demo wait', async () => {
		vi.useFakeTimers();
		try {
			demoEnabled.set(true);
			demoConfig.set({ ...testConfig, turn_pause_secs: 0 });
			streamingActive.set(true);
			const flush = vi.fn(() => {
				streamingActive.set(false);
				return 1;
			});
			flushStream.set(flush);

			const turn = runDemoTurn();
			await vi.advanceTimersByTimeAsync(10_000);
			await vi.runAllTimersAsync();
			await turn;

			expect(flush).toHaveBeenCalledTimes(1);
			expect(get(streamingActive)).toBe(false);
			expect(getLlmPlayerAction).toHaveBeenCalledTimes(1);
		} finally {
			vi.useRealTimers();
		}
	});

	// Regression for #999: `[system]` text-log entries that echo the
	// current location description must NOT appear in the `recent_log`
	// passed to `getLlmPlayerAction`. The static `location_description`
	// field already conveys the scene; duplicating it via recent_log
	// floods the prompt and anchors the auto-player on repeating itself.
	it('filters scene-description echoes out of recent_log', async () => {
		demoEnabled.set(true);
		textLog.set([
			{ id: 'a', source: 'player', content: 'Good morning' },
			// A `[system]` echo of the current scene description — must
			// be filtered.
			{ id: 'b', source: 'system', content: KILTEEVAN_SCENE },
			{ id: 'c', source: 'system', content: 'The clock chimes nine.' },
		]);

		await runDemoTurn();

		expect(getLlmPlayerAction).toHaveBeenCalledTimes(1);
		const ctxArg = vi.mocked(getLlmPlayerAction).mock.calls[0][0];
		expect(ctxArg.recent_log).toEqual([
			'[player] Good morning',
			'[system] The clock chimes nine.',
		]);
	});

	// Regression for #999: `recent_actions` must be populated from the
	// last `[player]` entries of the text log so the demo system prompt
	// can show the LLM what it has already said and break repetition.
	it('populates recent_actions from the last 5 player log entries', async () => {
		demoEnabled.set(true);
		textLog.set([
			{ id: 'p1', source: 'player', content: 'Good morning' },
			{ id: 's1', source: 'system', content: 'The clock chimes nine.' },
			{ id: 'p2', source: 'player', content: 'Good morning' },
			{ id: 'n1', source: 'Padraig', content: 'And to ye.' },
			{ id: 'p3', source: 'player', content: 'go to the mill' },
		]);

		await runDemoTurn();

		const ctxArg = vi.mocked(getLlmPlayerAction).mock.calls[0][0];
		expect(ctxArg.recent_actions).toEqual([
			'Good morning',
			'Good morning',
			'go to the mill',
		]);
	});

	// Regression for codex review on #1048: in long demo runs with heavy
	// NPC/system output, the last 5 `[player]` entries can sit OUTSIDE the
	// 40-entry recent_log window. recent_actions must therefore be derived
	// from the full textLog, not from the pre-truncated 40-entry slice, or
	// the anti-repetition signal silently stops working.
	it('derives recent_actions from full textLog even past the 40-entry recent_log window', async () => {
		demoEnabled.set(true);
		const log = [
			{ id: 'p1', source: 'player', content: 'first player utterance' },
			{ id: 'p2', source: 'player', content: 'second player utterance' },
		];
		// Pad with 50 NPC entries so the player entries fall outside any
		// 40-entry tail window.
		for (let i = 0; i < 50; i += 1) {
			log.push({ id: `n${i}`, source: 'Padraig', content: `npc line ${i}` });
		}
		textLog.set(log);

		await runDemoTurn();

		const ctxArg = vi.mocked(getLlmPlayerAction).mock.calls[0][0];
		expect(ctxArg.recent_actions).toEqual([
			'first player utterance',
			'second player utterance',
		]);
		// recent_log still honours the 40-entry cap.
		expect(ctxArg.recent_log.length).toBe(40);
	});

	// Regression for codex review on #1048: scene-echo filtering must happen
	// BEFORE the 40-entry truncation, or a stretch of scene-blurb spam at
	// the tail can consume the recent_log window and leave very little
	// usable history after the filter.
	it('filters scene echoes BEFORE truncating recent_log to 40 entries', async () => {
		demoEnabled.set(true);
		const log = [
			{ id: 'useful-1', source: 'Padraig', content: 'A useful old line.' },
			{ id: 'useful-2', source: 'player', content: 'A useful player line.' },
		];
		// Add 60 system scene echoes that should ALL be filtered.
		for (let i = 0; i < 60; i += 1) {
			log.push({ id: `echo-${i}`, source: 'system', content: KILTEEVAN_SCENE });
		}
		textLog.set(log);

		await runDemoTurn();

		const ctxArg = vi.mocked(getLlmPlayerAction).mock.calls[0][0];
		// Scene echoes all stripped; the two useful older entries survive.
		expect(ctxArg.recent_log).toEqual([
			'[Padraig] A useful old line.',
			'[player] A useful player line.',
		]);
	});
});
