import { get } from 'svelte/store';
import { textLog, streamingActive, languageHints, messageHints, trimTextLog } from '../../stores/game';
import { getStreamChunkDelayMs, takeNextStreamChunk } from '../stream-pacing';
import type { LanguageHint } from '../types';

export const STREAM_WAIT_FOR_WORD_MS = 70;

export type PendingNpcTurn = {
	turnId: number;
	source: string;
	messageId?: string;
	buffer: string;
	placeholderInserted: boolean;
	complete: boolean;
	pumpHandle: ReturnType<typeof setTimeout> | null;
};

export function appendStreamToken(turnId: number, source: string, token: string, messageId?: string) {
	textLog.update((log) => {
		const entryIndex = log.findIndex((entry) => entry.stream_turn_id === turnId);
		if (entryIndex >= 0) {
			const current = log[entryIndex];
			const nextEntry = {
				...current,
				id: current.id ?? messageId,
				source,
				content: current.content + token,
				stream_turn_id: turnId,
				streaming: true,
				latest_chunk: token,
				stream_chunk_id: (current.stream_chunk_id ?? 0) + 1
			};
			return [
				...log.slice(0, entryIndex),
				nextEntry,
				...log.slice(entryIndex + 1)
			];
		}
		return trimTextLog([
			...log,
			{
				id: messageId,
				source,
				content: token,
				stream_turn_id: turnId,
				streaming: true,
				latest_chunk: token,
				stream_chunk_id: 1
			}
		]);
	});
}

export interface StreamManager {
	findPendingTurn: (turnId: number) => PendingNpcTurn | undefined;
	queuePendingTurn: (turnId: number, source: string, messageId?: string) => PendingNpcTurn;
	ensureTurnEntry: (turn: PendingNpcTurn) => void;
	finalizeStreamingEntry: (turnId: number) => void;
	finishNpcStream: (hints?: LanguageHint[]) => void;
	maybeFinishNpcStream: () => void;
	stopTurnPump: (turn: PendingNpcTurn) => void;
	scheduleTurnPump: (turn: PendingNpcTurn, delayMs: number) => void;
	finalizePendingTurn: (turnId: number) => void;
	startTurnPumpIfNeeded: (turn: PendingNpcTurn) => void;
	pumpTurn: (turnId: number) => void;
	setPendingEndHints: (hints: LanguageHint[] | null) => void;
	pendingTurnCount: () => number;
	hasPendingEndHints: () => boolean;
	isChainInProgress: () => boolean;
	dispose: () => void;
}

export function createStreamManager(): StreamManager {
	let pendingNpcTurns = new Map<number, PendingNpcTurn>();
	let pendingStreamEndHints: LanguageHint[] | null = null;
	// True from the first stream-token of a conversation chain until
	// `finishNpcStream` runs. The +page.svelte `onLoading` handler reads
	// this to suppress mid-chain `loading {active:false}` events from
	// clearing `streamingActive` — handle_npc_conversation cancels and
	// re-spawns the loading animation per addressed NPC turn (#991).
	let chainInProgress = false;

	function findPendingTurn(turnId: number) {
		return pendingNpcTurns.get(turnId);
	}

	function queuePendingTurn(turnId: number, source: string, messageId?: string) {
		chainInProgress = true;
		const existing = findPendingTurn(turnId);
		if (existing) {
			existing.source = source;
			existing.messageId = existing.messageId ?? messageId;
			if (messageId && existing.placeholderInserted) {
				textLog.update((log) => {
					const entryIndex = log.findIndex((entry) => entry.stream_turn_id === turnId);
					if (entryIndex < 0) return log;
					return [
						...log.slice(0, entryIndex),
						{ ...log[entryIndex], id: log[entryIndex].id ?? messageId, source },
						...log.slice(entryIndex + 1)
					];
				});
			}
			return existing;
		}

		const turn: PendingNpcTurn = {
			turnId,
			source,
			messageId,
			buffer: '',
			placeholderInserted: false,
			complete: false,
			pumpHandle: null
		};
		pendingNpcTurns.set(turnId, turn);
		return turn;
	}

	function ensureTurnEntry(turn: PendingNpcTurn) {
		if (turn.placeholderInserted) return;

		textLog.update((log) =>
			trimTextLog([
				...log,
				{
					id: turn.messageId,
					source: turn.source,
					content: '',
					stream_turn_id: turn.turnId
				}
			])
		);
		turn.placeholderInserted = true;
	}

	function finalizeStreamingEntry(turnId: number) {
		textLog.update((log) => {
			const entryIndex = log.findIndex((entry) => entry.stream_turn_id === turnId);
			if (entryIndex < 0) {
				return log;
			}

			const entry = log[entryIndex];
			if (entry.content === '') {
				return [...log.slice(0, entryIndex), ...log.slice(entryIndex + 1)];
			}

			return [
				...log.slice(0, entryIndex),
				{
					...entry,
					streaming: false,
					latest_chunk: undefined,
					stream_chunk_id: undefined
				},
				...log.slice(entryIndex + 1)
			];
		});
	}

	function finishNpcStream(hints: LanguageHint[] = []) {
		if (hints.length > 0) {
			const log = get(textLog);
			for (let i = log.length - 1; i >= 0; i--) {
				if (log[i].id && log[i].source !== 'player' && log[i].source !== 'system') {
					messageHints.update((m) => { m.set(log[i].id!, hints); return m; });
					break;
				}
			}
		}
		languageHints.set(hints);
		streamingActive.set(false);
		chainInProgress = false;
	}

	function maybeFinishNpcStream() {
		if (pendingStreamEndHints === null || pendingNpcTurns.size > 0) return;
		finishNpcStream(pendingStreamEndHints);
		pendingStreamEndHints = null;
	}

	function stopTurnPump(turn: PendingNpcTurn) {
		if (turn.pumpHandle !== null) {
			clearTimeout(turn.pumpHandle);
			turn.pumpHandle = null;
		}
	}

	function scheduleTurnPump(turn: PendingNpcTurn, delayMs: number) {
		turn.pumpHandle = setTimeout(() => {
			turn.pumpHandle = null;
			pumpTurn(turn.turnId);
		}, delayMs);
	}

	function finalizePendingTurn(turnId: number) {
		const turn = findPendingTurn(turnId);
		if (!turn) return;
		stopTurnPump(turn);
		finalizeStreamingEntry(turnId);
		pendingNpcTurns.delete(turnId);
		maybeFinishNpcStream();
	}

	function startTurnPumpIfNeeded(turn: PendingNpcTurn) {
		if (turn.pumpHandle !== null) return;
		pumpTurn(turn.turnId);
	}

	function pumpTurn(turnId: number) {
		const turn = findPendingTurn(turnId);
		if (!turn) return;

		if (turn.buffer.length === 0) {
			stopTurnPump(turn);
			if (turn.complete) {
				finalizePendingTurn(turnId);
			}
			return;
		}

		ensureTurnEntry(turn);

		const { chunk, rest } = takeNextStreamChunk(turn.buffer, turn.complete);

		if (chunk === null) {
			scheduleTurnPump(turn, STREAM_WAIT_FOR_WORD_MS);
			return;
		}

		turn.buffer = rest;
		appendStreamToken(
			turn.turnId,
			turn.source,
			chunk,
			turn.messageId
		);
		scheduleTurnPump(turn, getStreamChunkDelayMs(chunk));
	}

	function setPendingEndHints(hints: LanguageHint[] | null) {
		pendingStreamEndHints = hints;
	}

	function pendingTurnCount() {
		return pendingNpcTurns.size;
	}

	function hasPendingEndHints() {
		return pendingStreamEndHints !== null;
	}

	function isChainInProgress() {
		return chainInProgress;
	}

	function dispose() {
		pendingNpcTurns.forEach((turn) => stopTurnPump(turn));
		pendingNpcTurns.clear();
		pendingStreamEndHints = null;
		chainInProgress = false;
	}

	return {
		findPendingTurn,
		queuePendingTurn,
		ensureTurnEntry,
		finalizeStreamingEntry,
		finishNpcStream,
		maybeFinishNpcStream,
		stopTurnPump,
		scheduleTurnPump,
		finalizePendingTurn,
		startTurnPumpIfNeeded,
		pumpTurn,
		setPendingEndHints,
		pendingTurnCount,
		hasPendingEndHints,
		isChainInProgress,
		dispose
	};
}
