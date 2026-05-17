import { describe, it, expect, beforeEach } from 'vitest';
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
