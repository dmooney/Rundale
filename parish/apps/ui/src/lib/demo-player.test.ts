import { describe, it, expect, beforeEach, vi } from 'vitest';
import { get } from 'svelte/store';
import {
	demoEnabled,
	demoPaused,
	demoStatus,
	demoTurnCount,
	demoConfig,
} from '../stores/demo';
import { streamingActive } from '../stores/game';
import { runDemoTurn, stopDemo } from './demo-player';
import { submitInput } from './ipc';
import { createStreamManager } from './setup/stream-manager';

const testConfig = {
	auto_start: false,
	extra_prompt: null,
	turn_pause_secs: 0.01,
	max_turns: null,
};

vi.mock('../lib/ipc', () => ({
	getDemoContext: vi.fn(async () => ({
		world_description: 'A village',
		recent_log: [],
		nearby_npcs: [],
		recent_events: [],
		extra_prompt: null,
	})),
	getLlmPlayerAction: vi.fn(async () => '"look around"'),
	submitInput: vi.fn(async () => {}),
}));

beforeEach(() => {
	demoEnabled.set(false);
	demoPaused.set(false);
	demoStatus.set('idle');
	demoTurnCount.set(0);
	demoConfig.set(testConfig);
	vi.mocked(submitInput).mockClear();
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
});
