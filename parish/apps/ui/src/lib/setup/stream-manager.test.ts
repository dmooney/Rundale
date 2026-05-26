import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { createStreamManager } from './stream-manager';
import { textLog, streamingActive, languageHints, messageHints } from '../../stores/game';

beforeEach(() => {
	textLog.set([]);
	streamingActive.set(false);
	languageHints.set([]);
	messageHints.set(new Map());
});

describe('createStreamManager — chainInProgress (#991)', () => {
	it('starts with chainInProgress false', () => {
		const sm = createStreamManager();
		expect(sm.isChainInProgress()).toBe(false);
	});

	it('sets chainInProgress true when the first stream-token queues a turn', () => {
		const sm = createStreamManager();
		expect(sm.isChainInProgress()).toBe(false);

		sm.queuePendingTurn(1, 'Padraig');
		expect(sm.isChainInProgress()).toBe(true);
	});

	it('stays true between per-turn finalisations within one chain', () => {
		const sm = createStreamManager();

		// Phase 1, NPC 1
		const t1 = sm.queuePendingTurn(1, 'Padraig');
		t1.buffer = 'Dia dhuit';
		t1.complete = true;
		sm.startTurnPumpIfNeeded(t1);
		sm.finalizePendingTurn(1);
		expect(sm.isChainInProgress()).toBe(true);

		// Phase 1, NPC 2
		const t2 = sm.queuePendingTurn(2, 'Siobhan');
		t2.buffer = 'Cén chaoi a bhfuil tu?';
		t2.complete = true;
		sm.startTurnPumpIfNeeded(t2);
		sm.finalizePendingTurn(2);
		expect(sm.isChainInProgress()).toBe(true);
	});

	it('resets to false only after finishNpcStream runs', () => {
		const sm = createStreamManager();

		const t1 = sm.queuePendingTurn(1, 'Padraig');
		t1.buffer = 'Hello';
		t1.complete = true;
		sm.startTurnPumpIfNeeded(t1);
		sm.finalizePendingTurn(1);
		expect(sm.isChainInProgress()).toBe(true);

		// stream-end arrives — sets pending hints, drains pending turns,
		// runs finishNpcStream.
		sm.setPendingEndHints([]);
		sm.maybeFinishNpcStream();
		expect(sm.isChainInProgress()).toBe(false);
	});

	it('finishNpcStream clears streamingActive and chainInProgress', () => {
		const sm = createStreamManager();
		streamingActive.set(true);
		sm.queuePendingTurn(1, 'Padraig');
		expect(sm.isChainInProgress()).toBe(true);

		sm.finishNpcStream([]);

		expect(get(streamingActive)).toBe(false);
		expect(sm.isChainInProgress()).toBe(false);
	});

	it('dispose resets chainInProgress', () => {
		const sm = createStreamManager();
		sm.queuePendingTurn(1, 'Padraig');
		expect(sm.isChainInProgress()).toBe(true);

		sm.dispose();
		expect(sm.isChainInProgress()).toBe(false);
	});

	// Regression for #45 (P0, user-reported): two NPC replies were
	// visibly being revealed at the same time during cycle 6 of the
	// demo audit because the backend spawns NPC replies in parallel
	// and the stream-manager's `pumpTurn` chain ran independently per
	// turn. The fix gates `pumpTurn` on a single `activeTurnId` head
	// so only one reveal is in flight at a time; parked turns
	// accumulate tokens in their buffer and drain after the head
	// finalises.
	describe('TODO #45 — single-active-turn serialisation', () => {
		beforeEach(() => {
			vi.useFakeTimers();
		});

		afterEach(() => {
			vi.useRealTimers();
		});

		it('only pumps the head turn while a second turn is parked', () => {
			const sm = createStreamManager();

			// Turn 1 arrives first — becomes head.
			const t1 = sm.queuePendingTurn(1, 'Padraig');
			t1.buffer = 'Hello there friend';
			sm.startTurnPumpIfNeeded(t1);
			// Head pump is running — pumpHandle is set after the first chunk
			// schedules the next tick (initial pumpTurn drains synchronously
			// then schedules).
			expect(t1.pumpHandle).not.toBeNull();

			// Turn 2 arrives while turn 1 is mid-reveal — must be parked,
			// not start a parallel pump.
			const t2 = sm.queuePendingTurn(2, 'Nora');
			t2.buffer = 'Aye it is';
			sm.startTurnPumpIfNeeded(t2);
			expect(t2.pumpHandle).toBeNull();

			// Parked turn keeps accumulating in its buffer (simulates more
			// tokens arriving via `appendStreamToken`).
			t2.buffer += ' so it is';
			expect(t2.pumpHandle).toBeNull();
		});

		it('promotes parked turn to head when active turn finalises', () => {
			const sm = createStreamManager();

			const t1 = sm.queuePendingTurn(1, 'Padraig');
			t1.buffer = 'Hi';
			t1.complete = true;
			sm.startTurnPumpIfNeeded(t1);

			const t2 = sm.queuePendingTurn(2, 'Nora');
			t2.buffer = 'Yes';
			t2.complete = true;
			sm.startTurnPumpIfNeeded(t2);
			expect(t2.pumpHandle).toBeNull();

			// Drain head: run all timers until turn 1's buffer empties +
			// turn 1 finalises. The promotion of turn 2 also fires.
			vi.runAllTimers();

			// Turn 1 fully removed; turn 2 either finished too or is mid-flight.
			expect(sm.findPendingTurn(1)).toBeUndefined();
			// Turn 2 should have drained (complete + buffered) — either
			// already removed, or its pump is still running.
			const t2After = sm.findPendingTurn(2);
			if (t2After) {
				// Pump must have started for turn 2 after promotion.
				expect(t2After.pumpHandle).not.toBeNull();
			}
		});

		it('parked turn buffer survives until promotion', () => {
			const sm = createStreamManager();

			const t1 = sm.queuePendingTurn(1, 'Padraig');
			t1.buffer = 'A';
			t1.complete = true;
			sm.startTurnPumpIfNeeded(t1);

			const t2 = sm.queuePendingTurn(2, 'Nora');
			// More tokens arrive on the parked turn while head is mid-reveal.
			t2.buffer += 'one ';
			t2.buffer += 'two ';
			t2.buffer += 'three';
			// Pump never started while parked.
			expect(t2.pumpHandle).toBeNull();
			// Buffer carries the full accumulation.
			expect(t2.buffer).toBe('one two three');
		});

		it('single-turn case is unchanged — pump starts immediately', () => {
			const sm = createStreamManager();
			const t1 = sm.queuePendingTurn(1, 'Padraig');
			t1.buffer = 'Solo';
			sm.startTurnPumpIfNeeded(t1);
			// Active head; pumpHandle is set.
			expect(t1.pumpHandle).not.toBeNull();
		});
	});

	// Regression for #991: simulates the bug sequence — loading=true,
	// stream-token (chain starts), turn drains, loading=false fires while
	// chain is mid-flight. The +page.svelte handler now consults
	// isChainInProgress() and must report true so streamingActive is NOT
	// cleared by the mid-chain loading=false.
	it('reports chainInProgress=true across the mid-chain loading=false window', () => {
		const sm = createStreamManager();
		streamingActive.set(true); // loading.active=true was just observed

		// NPC 1 tokens
		const t1 = sm.queuePendingTurn(1, 'Padraig');
		t1.buffer = 'Hello!';
		t1.complete = true;
		sm.startTurnPumpIfNeeded(t1);
		sm.finalizePendingTurn(1);

		// At this point the backend would cancel its per-turn loading
		// animation and emit `loading {active:false}`. The handler's gate
		// must see isChainInProgress=true and skip the streamingActive
		// reset.
		expect(sm.isChainInProgress()).toBe(true);
		expect(sm.pendingTurnCount()).toBe(0);
		expect(sm.hasPendingEndHints()).toBe(false);

		// The chain continues: autonomous follow-up turn arrives.
		const t2 = sm.queuePendingTurn(2, 'Siobhan');
		t2.buffer = 'Indeed!';
		t2.complete = true;
		sm.startTurnPumpIfNeeded(t2);
		sm.finalizePendingTurn(2);

		// Chain ends.
		sm.setPendingEndHints([]);
		sm.maybeFinishNpcStream();

		expect(sm.isChainInProgress()).toBe(false);
		expect(get(streamingActive)).toBe(false);
	});
});
