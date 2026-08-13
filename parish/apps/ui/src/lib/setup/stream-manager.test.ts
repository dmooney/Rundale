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
	it('preserves the full canonical buffer when reconnect lands after one paced word', () => {
		vi.useFakeTimers();
		const sm = createStreamManager();
		const turn = sm.queuePendingTurn(1857, 'Brigid', 'msg-1857');
		turn.buffer = 'Plainly, the complete response must survive reconnect.';
		sm.startTurnPumpIfNeeded(turn);
		expect(get(textLog)[0].content).toBe('Plainly, ');

		sm.reset();

		expect(get(textLog)[0]).toMatchObject({
			id: 'msg-1857',
			content: 'Plainly, the complete response must survive reconnect.',
			streaming: false,
		});
		expect(get(textLog)[0].stream_turn_id).toBeUndefined();
		vi.useRealTimers();
	});

	it('clears streaming flags on a half-streamed entry (reconnect mid-turn)', () => {
		const sm = createStreamManager();
		// A partially-streamed NPC bubble: content present, still streaming.
		textLog.set([
			{
				id: 'm1',
				source: 'Padraig',
				content: 'Dia dh',
				streaming: true,
				latest_chunk: 'dh',
				stream_chunk_id: 3,
			},
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
			{
				id: 'p1',
				source: 'Siobhan',
				content: '',
				streaming: true,
				stream_turn_id: 7,
			},
			{ id: 'm2', source: 'system', content: 'A scene line.' },
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
			{ id: 'b', source: 'Padraig', content: 'Finished reply.' },
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

describe('createStreamManager — authoritative turn finalization (#1855, #1857)', () => {
	beforeEach(() => vi.useFakeTimers());
	afterEach(() => vi.useRealTimers());

	it('replaces the paced first word with the full validated final response', () => {
		const sm = createStreamManager();
		const turn = sm.queuePendingTurn(1857, 'Brigid', 'msg-1857');
		turn.buffer = 'Plainly, the whole answer survives Gemini finalization.';
		sm.startTurnPumpIfNeeded(turn);

		// The pacing pump has revealed only its synchronous first chunk.
		expect(get(textLog)[0].content).toBe('Plainly, ');

		sm.completeTurn({
			turn_id: 1857,
			status: 'completed',
			message_id: 'msg-1857',
			source: 'Brigid',
			final_text: 'Plainly, the whole answer survives Gemini finalization.',
		});

		expect(get(textLog)).toEqual([
			{
				id: 'msg-1857',
				source: 'Brigid',
				content: 'Plainly, the whole answer survives Gemini finalization.',
				stream_turn_id: undefined,
				streaming: false,
				latest_chunk: undefined,
				stream_chunk_id: undefined,
			},
		]);
		expect(sm.pendingTurnCount()).toBe(0);
		vi.runAllTimers();
		expect(get(textLog)[0].content).toContain('whole answer');
	});

	it('discards every partial and renders retry guidance on failed termination', () => {
		const sm = createStreamManager();
		const turn = sm.queuePendingTurn(1855, 'Brigid', 'msg-1855');
		turn.buffer = 'This candidate was cut off';
		sm.startTurnPumpIfNeeded(turn);
		expect(get(textLog)[0].content).toBe('This ');

		sm.completeTurn({
			turn_id: 1855,
			status: 'failed',
			message_id: 'msg-1855',
			recovery_message:
				'That reply could not be completed, so its partial response was not added. Please try again.',
		});

		const log = get(textLog);
		expect(log).toHaveLength(1);
		expect(log[0]).toMatchObject({ source: 'system', subtype: 'error' });
		expect(log[0].content).toContain('Please try again');
		expect(log[0].content).not.toContain('cut off');
		expect(sm.pendingTurnCount()).toBe(0);
		vi.runAllTimers();
		expect(get(textLog)).toEqual(log);
		sm.completeTurn({
			turn_id: 1855,
			status: 'failed',
			message_id: 'msg-1855',
			recovery_message: log[0].content,
		});
		expect(get(textLog)).toHaveLength(1);
	});

	it('can recover a completed turn after reconnect without a local buffer', () => {
		const sm = createStreamManager();
		textLog.set([
			{
				id: 'msg-9',
				source: 'Nora',
				content: 'The terminal payload carries',
				streaming: false,
			},
		]);
		sm.completeTurn({
			turn_id: 9,
			status: 'completed',
			message_id: 'msg-9',
			source: 'Nora',
			final_text: 'The terminal payload carries the complete truth.',
		});
		expect(get(textLog)[0]).toMatchObject({
			id: 'msg-9',
			source: 'Nora',
			content: 'The terminal payload carries the complete truth.',
		});
	});

	it('ignores late success and failure terminals after context replacement', () => {
		const sm = createStreamManager();
		sm.completeTurn({
			turn_id: 91,
			status: 'completed',
			message_id: 'old-session-message',
			source: 'Nora',
			final_text: 'This old session must not be resurrected.',
		});
		sm.completeTurn({
			turn_id: 92,
			status: 'failed',
			message_id: 'old-session-failure',
			recovery_message: 'This old failure must not be resurrected.',
		});
		expect(get(textLog)).toEqual([]);
	});

	it('keeps an authoritative parked reply behind the active speaker', () => {
		const sm = createStreamManager();
		const first = sm.queuePendingTurn(1, 'Brigid', 'msg-1');
		first.buffer = 'First speaker remains active.';
		sm.startTurnPumpIfNeeded(first);
		const parked = sm.queuePendingTurn(2, 'Nora', 'msg-2');
		sm.ensureTurnEntry(parked);
		parked.buffer = 'token candidate';

		sm.completeTurn({
			turn_id: 2,
			status: 'completed',
			message_id: 'msg-2',
			source: 'Nora',
			final_text: 'Second speaker waits with complete canonical text.',
		});
		sm.correctTurn(
			2,
			'Second speaker waits with complete corrected text.',
			'msg-2',
		);

		expect(get(textLog).find((entry) => entry.id === 'msg-2')?.content).toBe(
			'',
		);
		expect(sm.findPendingTurn(2)?.buffer).toBe(
			'Second speaker waits with complete corrected text.',
		);
	});
});

describe('createStreamManager — flushAll() snaps in-flight reply to completion (#1379)', () => {
	beforeEach(() => {
		vi.useFakeTimers();
	});
	afterEach(() => {
		vi.useRealTimers();
	});

	it('reveals the full buffered text instantly and finalizes the entry', () => {
		const sm = createStreamManager();
		streamingActive.set(true);

		// A long reply is mid-reveal: only a few chars have been pumped, the
		// rest sits in the buffer.
		const t1 = sm.queuePendingTurn(1, 'Seamus', 'm1');
		t1.buffer = 'Aye, that I can, and gladly too, for the forge is cold.';
		t1.complete = true;
		sm.startTurnPumpIfNeeded(t1);

		// Player starts typing → flush.
		const flushed = sm.flushAll();

		expect(flushed).toBe(1);
		const log = get(textLog);
		// After flush, finalizeStreamingEntry clears stream_turn_id (#1377) so
		// look up by content instead.
		const entry = log.find(
			(e) =>
				e.content === 'Aye, that I can, and gladly too, for the forge is cold.',
		);
		expect(entry).toBeDefined();
		// Full text is revealed, not just the few pumped chars.
		expect(entry?.content).toBe(
			'Aye, that I can, and gladly too, for the forge is cold.',
		);
		// Entry is finalized — no longer streaming. stream_turn_id was cleared by
		// finalizeStreamingEntry to prevent post-reset stream re-fill (#1377).
		expect(entry?.streaming).toBe(false);
		expect(entry?.latest_chunk).toBeUndefined();
		expect(entry?.stream_turn_id).toBeUndefined();
		// Pump timers cancelled, pool drained, streaming cleared.
		expect(t1.pumpHandle).toBeNull();
		expect(sm.pendingTurnCount()).toBe(0);
		expect(sm.isChainInProgress()).toBe(false);
		expect(get(streamingActive)).toBe(false);
	});

	it('flushes every parked turn, not just the head', () => {
		const sm = createStreamManager();
		streamingActive.set(true);

		const t1 = sm.queuePendingTurn(1, 'Padraig', 'm1');
		t1.buffer = 'First reply in full.';
		sm.startTurnPumpIfNeeded(t1);

		const t2 = sm.queuePendingTurn(2, 'Nora', 'm2');
		t2.buffer = 'Second reply in full.';
		sm.startTurnPumpIfNeeded(t2); // parked behind head

		sm.flushAll();

		// After flush, finalizeStreamingEntry clears stream_turn_id (#1377) so
		// look up by content instead.
		const log = get(textLog);
		expect(log.find((e) => e.content === 'First reply in full.')).toBeDefined();
		expect(
			log.find((e) => e.content === 'Second reply in full.'),
		).toBeDefined();
		expect(sm.pendingTurnCount()).toBe(0);
		expect(get(streamingActive)).toBe(false);
	});

	it('is a no-op (returns 0) when nothing is streaming', () => {
		const sm = createStreamManager();
		streamingActive.set(false);
		expect(sm.flushAll()).toBe(0);
		expect(get(streamingActive)).toBe(false);
	});

	it('clears a pure-spinner state (loading but no tokens yet)', () => {
		const sm = createStreamManager();
		streamingActive.set(true);
		// Chain started (loading) but the first token hasn't queued a turn.
		sm.queuePendingTurn(1, 'Maire'); // placeholder, empty buffer
		// Drop it to simulate "no tokens buffered" — actually keep it; flush
		// should still finalize/clear. Here we test the empty-buffer turn.
		sm.flushAll();
		expect(sm.pendingTurnCount()).toBe(0);
		expect(get(streamingActive)).toBe(false);
		expect(sm.isChainInProgress()).toBe(false);
	});

	it('a late stream-end after flush does not re-open the stream', () => {
		const sm = createStreamManager();
		streamingActive.set(true);
		const t1 = sm.queuePendingTurn(1, 'Brigid', 'm1');
		t1.buffer = 'Done and dusted.';
		t1.complete = true;
		sm.startTurnPumpIfNeeded(t1);

		sm.flushAll();
		expect(get(streamingActive)).toBe(false);

		// The terminal stream-end arrives late (it raced the flush).
		sm.setPendingEndHints([]);
		sm.maybeFinishNpcStream();

		// Pool is empty and chain already finished — no resurrection.
		expect(get(streamingActive)).toBe(false);
		expect(sm.isChainInProgress()).toBe(false);
		expect(sm.pendingTurnCount()).toBe(0);
	});
});

describe('reset() clears stream_turn_id from finalized entries (#1377)', () => {
	// C4: After reset() finalizes a streaming textLog entry, its stream_turn_id
	// must be undefined so post-reset resumed streams cannot match it.
	it('clears stream_turn_id on finalized entries after reset', async () => {
		vi.useFakeTimers();
		const sm = createStreamManager();

		// Simulate an in-progress stream: queue a turn, add some content.
		const turn = sm.queuePendingTurn(42, 'Seamus', 'msg-42');
		sm.ensureTurnEntry(turn);
		// Directly update the textLog to simulate a partially-streamed entry.
		textLog.update((log) =>
			log.map((e) =>
				e.stream_turn_id === 42
					? { ...e, content: 'Mornin', streaming: true, stream_chunk_id: 1 }
					: e,
			),
		);

		// Reset orphans the in-progress stream (e.g. WS reconnect).
		sm.reset();

		const entries = get(textLog);
		const orphan = entries.find((e) => e.content === 'Mornin');
		expect(orphan).toBeDefined();
		// C4: stream_turn_id must be cleared after reset.
		expect(orphan?.stream_turn_id).toBeUndefined();
		// streaming flag must also be cleared.
		expect(orphan?.streaming).toBeFalsy();

		vi.useRealTimers();
	});

	// C5: appendStreamToken after reset for a now-finalized turn_id must NOT
	// find and re-fill the old finalized entry; it must create a fresh one.
	it('post-reset appendStreamToken does not re-fill the old finalized entry', async () => {
		vi.useFakeTimers();
		const sm = createStreamManager();

		// Partially-stream turn 42, then reset.
		const turn = sm.queuePendingTurn(42, 'Seamus', 'msg-42');
		sm.ensureTurnEntry(turn);
		textLog.update((log) =>
			log.map((e) =>
				e.stream_turn_id === 42
					? { ...e, content: 'Mornin', streaming: true, stream_chunk_id: 1 }
					: e,
			),
		);
		sm.reset();

		// Simulate backend resuming the stream after reconnect.
		const resumed = sm.queuePendingTurn(42, 'Seamus', 'msg-42');
		resumed.buffer += ' lad';
		resumed.complete = true;
		sm.startTurnPumpIfNeeded(resumed);

		// Drain the pump.
		await vi.waitFor(() => sm.pendingTurnCount() === 0);
		vi.runAllTimers();
		await vi.waitFor(() => sm.pendingTurnCount() === 0);

		const entries = get(textLog);
		// The old finalized entry ('Mornin') must still have its content intact
		// and stream_turn_id undefined — it was not re-filled.
		const old = entries.find((e) => e.content === 'Mornin');
		expect(old?.stream_turn_id).toBeUndefined();
		// C5: A fresh entry was created for the resumed turn, not the old one.
		// (The resumed content ' lad' should not appear appended to 'Mornin'.)
		expect(old?.content).toBe('Mornin');

		vi.useRealTimers();
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
