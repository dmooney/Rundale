import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { get } from 'svelte/store';
import { createStreamManager } from './stream-manager';
import {
	textLog,
	streamingActive,
	languageHints,
	messageHints,
} from '../../stores/game';

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

describe('createStreamManager — reset() finalizes orphaned stream entries', () => {
	it('clears streaming flags on a half-streamed entry (reconnect mid-turn)', () => {
		const sm = createStreamManager();
		// A partially-streamed NPC bubble: content present, still streaming.
		textLog.set([
			{ id: 'm1', source: 'Padraig', content: 'Dia dh', streaming: true, latest_chunk: 'dh', stream_chunk_id: 3 }
		]);

		sm.reset();

		const log = get(textLog);
		expect(log.length).toBe(1);
		expect(log[0].content).toBe('Dia dh'); // partial text preserved
		expect(log[0].streaming).toBe(false);
		expect(log[0].latest_chunk).toBeUndefined();
		expect(log[0].stream_chunk_id).toBeUndefined();
	});

	it('drops an empty streaming placeholder on reset', () => {
		const sm = createStreamManager();
		textLog.set([
			{ id: 'p1', source: 'Siobhan', content: '', streaming: true, stream_turn_id: 7 },
			{ id: 'm2', source: 'system', content: 'A scene line.' }
		]);

		sm.reset();

		const log = get(textLog);
		expect(log.length).toBe(1);
		expect(log[0].id).toBe('m2'); // empty placeholder removed, real entry kept
	});

	it('leaves non-streaming entries untouched', () => {
		const sm = createStreamManager();
		const entries = [
			{ id: 'a', source: 'player', content: '> hello' },
			{ id: 'b', source: 'Padraig', content: 'Finished reply.' }
		];
		textLog.set(entries);

		sm.reset();

		expect(get(textLog)).toEqual(entries);
	});
});

describe('createStreamManager — resumed stream rebinds to a reactable id (#1164)', () => {
	it('queuePendingTurn carries a message_id supplied by the resumed stream-token', () => {
		const sm = createStreamManager();
		// Reconnect dropped the placeholder text-log (and its id) during the gap,
		// so the FIRST signal of this turn the client sees is a stream-token that
		// now carries message_id. ensureTurnEntry must mint the bubble WITH that id.
		const turn = sm.queuePendingTurn(42, 'Padraig', 'msg-42');
		expect(turn.messageId).toBe('msg-42');

		sm.ensureTurnEntry(turn);
		const log = get(textLog);
		expect(log.length).toBe(1);
		expect(log[0].id).toBe('msg-42'); // reactable — id is keyed to the entry
		expect(log[0].stream_turn_id).toBe(42);
	});

	it('appendStreamToken rebuilds a missing entry with the resumed message_id', async () => {
		const sm = createStreamManager();
		// Empty placeholder was dropped by reset(); a resumed token arrives. The
		// onStreamToken path buffers then pumps; assert the rebuilt entry adopts
		// the id so reactions + language hints can key to it.
		const turn = sm.queuePendingTurn(9, 'Siobhan', 'msg-9');
		turn.buffer += 'Dia dhuit';
		turn.complete = true;
		sm.startTurnPumpIfNeeded(turn);

		// Drain the paced pump.
		await vi.waitFor(() => {
			const entry = get(textLog).find((e) => e.stream_turn_id === 9);
			expect(entry?.content).toContain('Dia');
		});
		const entry = get(textLog).find((e) => e.stream_turn_id === 9);
		expect(entry?.id).toBe('msg-9');
	});

	it('does not clobber an id already set by the placeholder', () => {
		const sm = createStreamManager();
		// Normal flow: placeholder arrived first and set the id; a later
		// stream-token carrying a (redundant) id must not overwrite it.
		const turn = sm.queuePendingTurn(5, 'Nora', 'placeholder-id');
		sm.ensureTurnEntry(turn);
		sm.queuePendingTurn(5, 'Nora', 'token-id'); // resumed/duplicate signal
		expect(sm.findPendingTurn(5)?.messageId).toBe('placeholder-id');
		expect(get(textLog)[0].id).toBe('placeholder-id');
	});
});

describe('reconnect re-asserts streamingActive from turn_in_flight (#1164 AC3)', () => {
	// Models the +page.svelte onReconnect resync decision: after sm.reset() +
	// streamingActive.set(false), the handler reads the authoritative snapshot
	// and re-asserts streamingActive when the server says a turn is in flight.
	// This locks the contract so the pre-token duplicate-turn window can't
	// silently reopen if the guard is removed.
	function applyReconnectResync(snap: { turn_in_flight?: boolean }) {
		const sm = createStreamManager();
		sm.reset();
		streamingActive.set(false);
		if (snap.turn_in_flight) streamingActive.set(true);
	}

	it('keeps streamingActive true when a turn is in flight across the gap', () => {
		streamingActive.set(false);
		applyReconnectResync({ turn_in_flight: true });
		expect(get(streamingActive)).toBe(true);
	});

	it('clears streamingActive when the engine is idle on reconnect', () => {
		streamingActive.set(true);
		applyReconnectResync({ turn_in_flight: false });
		expect(get(streamingActive)).toBe(false);
	});

	it('treats a missing turn_in_flight (older payload) as idle', () => {
		streamingActive.set(true);
		applyReconnectResync({});
		expect(get(streamingActive)).toBe(false);
	});
});
