import { get } from 'svelte/store';
import {
	textLog,
	streamingActive,
	languageHints,
	messageHints,
	trimTextLog,
} from '../../stores/game';
import { getStreamChunkDelayMs, takeNextStreamChunk } from '../stream-pacing';
import type { LanguageHint, StreamTurnEndPayload } from '../types';

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

export function appendStreamToken(
	turnId: number,
	source: string,
	token: string,
	messageId?: string,
) {
	textLog.update((log) => {
		const entryIndex = log.findIndex(
			(entry) => entry.stream_turn_id === turnId,
		);
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
				stream_chunk_id: (current.stream_chunk_id ?? 0) + 1,
			};
			return [
				...log.slice(0, entryIndex),
				nextEntry,
				...log.slice(entryIndex + 1),
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
				stream_chunk_id: 1,
			},
		]);
	});
}

export interface StreamManager {
	findPendingTurn: (turnId: number) => PendingNpcTurn | undefined;
	queuePendingTurn: (
		turnId: number,
		source: string,
		messageId?: string,
	) => PendingNpcTurn;
	ensureTurnEntry: (turn: PendingNpcTurn) => void;
	finishNpcStream: (hints?: LanguageHint[]) => void;
	maybeFinishNpcStream: () => void;
	finalizePendingTurn: (turnId: number) => void;
	/** Applies the backend's authoritative terminal truth. Successful dialogue
	 *  replaces any paced prefix with the complete validated text; failures
	 *  discard every partial and surface retry guidance. */
	completeTurn: (payload: StreamTurnEndPayload) => void;
	/** Applies the legacy post-guard correction without allowing a parked NPC
	 * turn to bypass single-speaker reveal ordering. */
	correctTurn: (
		turnId: number,
		correctedText: string,
		messageId?: string,
	) => void;
	startTurnPumpIfNeeded: (turn: PendingNpcTurn) => void;
	/** Immediately reveals the full text of every in-flight streamed NPC line:
	 *  drains each pending turn's buffer into its textLog entry, cancels the
	 *  token-by-token pump timers, finalizes the entries (clears `streaming`),
	 *  and clears `streamingActive`. Triggered when the player starts typing the
	 *  next input so the current reply snaps fully into view before the next
	 *  turn is accepted (#1379). Returns the number of turns that were flushed.
	 *  A no-op (returns 0) when nothing is streaming. */
	flushAll: () => number;
	setPendingEndHints: (hints: LanguageHint[] | null) => void;
	pendingTurnCount: () => number;
	hasPendingEndHints: () => boolean;
	isChainInProgress: () => boolean;
	/** Finalizes canonical buffered text, then clears in-flight stream state
	 * without tearing the manager down. Used on reconnect and context reset;
	 * the latter clears the transcript immediately afterward. */
	reset: () => void;
	dispose: () => void;
}

export function createStreamManager(): StreamManager {
	const pendingNpcTurns = new Map<number, PendingNpcTurn>();
	let pendingStreamEndHints: LanguageHint[] | null = null;
	// This store admits only the canonical `stream-token` protocol; unvalidated
	// provider candidate tokens never reach it (#1834). True from the first
	// stream-token of a conversation chain until
	// `finishNpcStream` runs. The +page.svelte `onLoading` handler reads
	// this to suppress mid-chain `loading {active:false}` events from
	// clearing `streamingActive` — handle_npc_conversation cancels and
	// re-spawns the loading animation per addressed NPC turn (#991).
	let chainInProgress = false;
	// TODO #45: serialise reveal across multiple in-flight NPC turns.
	// The backend spawns NPC replies in parallel (`tokio::spawn` per
	// addressee), so two streams can land at once. Only the head turn
	// pumps tokens; non-head turns buffer in `PendingNpcTurn.buffer`
	// (set by appendStreamToken on the parked entry's stream_turn_id
	// — the textLog entry still updates so the placeholder advances
	// silently, but the visible reveal stays single-threaded by
	// gating `pumpTurn` on activeTurnId).
	let activeTurnId: number | null = null;

	function findPendingTurn(turnId: number) {
		return pendingNpcTurns.get(turnId);
	}

	function queuePendingTurn(
		turnId: number,
		source: string,
		messageId?: string,
	) {
		chainInProgress = true;
		const existing = findPendingTurn(turnId);
		if (existing) {
			existing.source = source;
			existing.messageId = existing.messageId ?? messageId;
			if (messageId && existing.placeholderInserted) {
				textLog.update((log) => {
					const entryIndex = log.findIndex(
						(entry) => entry.stream_turn_id === turnId,
					);
					if (entryIndex < 0) return log;
					return [
						...log.slice(0, entryIndex),
						{ ...log[entryIndex], id: log[entryIndex].id ?? messageId, source },
						...log.slice(entryIndex + 1),
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
			pumpHandle: null,
		};
		pendingNpcTurns.set(turnId, turn);
		// TODO #45: first turn into an empty pool becomes the head.
		// Later arrivals park until the head finalises and promotes the
		// next FIFO entry.
		if (activeTurnId === null) {
			activeTurnId = turnId;
		}
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
					stream_turn_id: turn.turnId,
				},
			]),
		);
		turn.placeholderInserted = true;
	}

	function finalizeStreamingEntry(turnId: number) {
		textLog.update((log) => {
			const entryIndex = log.findIndex(
				(entry) => entry.stream_turn_id === turnId,
			);
			if (entryIndex < 0) {
				return log;
			}

			const entry = log[entryIndex];
			if (entry.content === '') {
				return [...log.slice(0, entryIndex), ...log.slice(entryIndex + 1)];
			}

			// Clear stream_turn_id so that a post-reset resumed stream for the
			// same turn_id cannot match this already-finalized entry via
			// appendStreamToken and re-fill it, which would produce a duplicate
			// dialogue bubble (#1377).
			return [
				...log.slice(0, entryIndex),
				{
					...entry,
					streaming: false,
					latest_chunk: undefined,
					stream_chunk_id: undefined,
					stream_turn_id: undefined,
				},
				...log.slice(entryIndex + 1),
			];
		});
	}

	function finishNpcStream(hints: LanguageHint[] = []) {
		if (hints.length > 0) {
			const log = get(textLog);
			for (let i = log.length - 1; i >= 0; i--) {
				if (
					log[i].id &&
					log[i].source !== 'player' &&
					log[i].source !== 'system'
				) {
					messageHints.update((m) => {
						m.set(log[i].id!, hints);
						return m;
					});
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
		// TODO #45: if the head finalises, promote the next parked turn
		// (insertion order). Non-head finalises don't disturb the head.
		if (activeTurnId === turnId) {
			activeTurnId = nextParkedTurnId();
			if (activeTurnId !== null) {
				const next = pendingNpcTurns.get(activeTurnId);
				if (next) {
					pumpTurn(next.turnId);
				}
			}
		}
		maybeFinishNpcStream();
	}

	function completeTurn(payload: StreamTurnEndPayload) {
		const turn = findPendingTurn(payload.turn_id);

		// Arrival-reaction streams have no canonical message identity/final text.
		// Preserve their paced-token completion behavior.
		if (
			(payload.status === 'completed' || payload.status === undefined) &&
			payload.final_text === undefined
		) {
			if (!turn) return;
			turn.complete = true;
			startTurnPumpIfNeeded(turn);
			return;
		}

		if (turn) stopTurnPump(turn);
		if (
			turn &&
			payload.status === 'completed' &&
			payload.final_text !== undefined &&
			turn.turnId !== activeTurnId
		) {
			turn.source = payload.source ?? turn.source;
			turn.messageId = turn.messageId ?? payload.message_id;
			turn.buffer = payload.final_text;
			turn.complete = true;
			return;
		}
		textLog.update((log) => {
			const recoveryId = payload.message_id
				? `${payload.message_id}:error`
				: undefined;
			const index = log.findIndex(
				(entry) =>
					entry.stream_turn_id === payload.turn_id ||
					(payload.message_id !== undefined && entry.id === payload.message_id),
			);

			if (payload.status === 'failed') {
				if (
					recoveryId !== undefined &&
					log.some((entry) => entry.id === recoveryId)
				) {
					return log;
				}
				// A context replacement clears both pending turns and transcript
				// identities. Ignore an old-session terminal that arrives afterward.
				if (!turn && index < 0) return log;
				const withoutPartial =
					index < 0 ? log : [...log.slice(0, index), ...log.slice(index + 1)];
				return payload.recovery_message
					? trimTextLog([
							...withoutPartial,
							{
								id: recoveryId,
								source: 'system',
								subtype: 'error',
								content: payload.recovery_message,
							},
						])
					: withoutPartial;
			}

			// Same rule for successful late terminals: a same-context reconnect
			// retains the finalized message ID, while a replacement context does not.
			if (!turn && index < 0) return log;

			const completed = {
				...(index >= 0 ? log[index] : {}),
				id: payload.message_id ?? (index >= 0 ? log[index].id : undefined),
				source:
					payload.source ??
					turn?.source ??
					(index >= 0 ? log[index].source : 'NPC'),
				content: payload.final_text ?? '',
				stream_turn_id: undefined,
				streaming: false,
				latest_chunk: undefined,
				stream_chunk_id: undefined,
			};
			if (index < 0) return trimTextLog([...log, completed]);
			return [...log.slice(0, index), completed, ...log.slice(index + 1)];
		});
		if (turn) finalizePendingTurn(payload.turn_id);
		else maybeFinishNpcStream();
	}

	function correctTurn(
		turnId: number,
		correctedText: string,
		messageId?: string,
	) {
		const turn = findPendingTurn(turnId);
		if (turn && turn.turnId !== activeTurnId) {
			stopTurnPump(turn);
			turn.messageId = turn.messageId ?? messageId;
			turn.buffer = correctedText;
			turn.complete = true;
			return;
		}
		completeTurn({
			turn_id: turnId,
			status: 'completed',
			message_id: messageId ?? turn?.messageId,
			source: turn?.source,
			final_text: correctedText,
		});
	}

	function nextParkedTurnId(): number | null {
		const iter = pendingNpcTurns.keys().next();
		return iter.done ? null : iter.value;
	}

	function startTurnPumpIfNeeded(turn: PendingNpcTurn) {
		if (turn.pumpHandle !== null) return;
		// TODO #45: only the head turn pumps. Parked turns accumulate
		// tokens in their buffer; they'll be drained when promoted by
		// the head's `finalizePendingTurn`.
		if (turn.turnId !== activeTurnId) return;
		pumpTurn(turn.turnId);
	}

	function pumpTurn(turnId: number) {
		const turn = findPendingTurn(turnId);
		if (!turn) return;
		// TODO #45: gate visible reveal on activeTurnId. A late timer
		// firing for an already-demoted turn (race between scheduleTurnPump
		// and finalizePendingTurn) must not draw chunks for a non-head
		// turn — would resurrect the parallel-reveal bug.
		if (turn.turnId !== activeTurnId) {
			stopTurnPump(turn);
			return;
		}

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
		appendStreamToken(turn.turnId, turn.source, chunk, turn.messageId);
		scheduleTurnPump(turn, getStreamChunkDelayMs(chunk));
	}

	function flushAll(): number {
		if (pendingNpcTurns.size === 0) {
			// Nothing buffered. The loading spinner may still be up (tokens not
			// yet arrived) — only clear streamingActive if a chain was genuinely
			// in progress, leaving the idle case untouched.
			if (chainInProgress) {
				finishNpcStream(pendingStreamEndHints ?? []);
				pendingStreamEndHints = null;
			}
			return 0;
		}

		let flushed = 0;
		// Snapshot keys: finalizePendingTurn mutates the map as it promotes turns.
		for (const turnId of [...pendingNpcTurns.keys()]) {
			const turn = pendingNpcTurns.get(turnId);
			if (!turn) continue;
			stopTurnPump(turn);
			// Reveal the full buffered text at once. ensureTurnEntry guarantees a
			// log entry exists even for a turn that had only its placeholder.
			if (turn.buffer.length > 0) {
				ensureTurnEntry(turn);
				appendStreamToken(
					turn.turnId,
					turn.source,
					turn.buffer,
					turn.messageId,
				);
				turn.buffer = '';
			}
			finalizeStreamingEntry(turnId);
			pendingNpcTurns.delete(turnId);
			flushed += 1;
		}
		activeTurnId = null;

		// Apply whatever end-hints we already have (the terminal stream-end may
		// not have arrived yet; that's fine — a late stream-end finds an empty
		// pool and maybeFinishNpcStream is a no-op since chainInProgress is now
		// cleared by finishNpcStream).
		finishNpcStream(pendingStreamEndHints ?? []);
		pendingStreamEndHints = null;
		return flushed;
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

	function reset() {
		// Since provider candidates are quarantined, every buffered stream-token is
		// already canonical player text. Preserve its complete batch before a
		// same-context reconnect discards timers; otherwise the paced first word
		// would become permanent when the terminal event was lost in the gap.
		if (pendingNpcTurns.size > 0) flushAll();
		pendingNpcTurns.forEach((turn) => stopTurnPump(turn));
		pendingNpcTurns.clear();
		pendingStreamEndHints = null;
		chainInProgress = false;
		activeTurnId = null;
		// Finalize any half-streamed log entry orphaned by the reset (e.g. a
		// reconnect after stream-token but before stream-end): without this the
		// entry keeps `streaming: true`/`latest_chunk` forever — a frozen cursor
		// bubble with reactions disabled. Clear the streaming flags (or drop an
		// empty placeholder), mirroring finalizeStreamingEntry across all turns.
		textLog.update((log) => {
			let changed = false;
			const out: typeof log = [];
			for (const entry of log) {
				const isStreaming =
					entry.streaming ||
					entry.latest_chunk !== undefined ||
					entry.stream_chunk_id !== undefined;
				if (!isStreaming) {
					out.push(entry);
					continue;
				}
				changed = true;
				if (entry.content === '') continue; // drop empty placeholder
				// Also clear stream_turn_id so a post-reset resumed stream
				// cannot match this finalized entry by turn id and re-fill it,
				// which would produce a duplicate dialogue bubble (#1377).
				out.push({
					...entry,
					streaming: false,
					latest_chunk: undefined,
					stream_chunk_id: undefined,
					stream_turn_id: undefined,
				});
			}
			return changed ? out : log;
		});
	}

	function dispose() {
		reset();
	}

	return {
		findPendingTurn,
		queuePendingTurn,
		ensureTurnEntry,
		finishNpcStream,
		maybeFinishNpcStream,
		finalizePendingTurn,
		completeTurn,
		correctTurn,
		startTurnPumpIfNeeded,
		flushAll,
		setPendingEndHints,
		pendingTurnCount,
		hasPendingEndHints,
		isChainInProgress,
		reset,
		dispose,
	};
}
