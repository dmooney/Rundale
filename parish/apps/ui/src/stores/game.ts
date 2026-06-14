import { writable, get } from 'svelte/store';
import type {
	WorldSnapshot,
	MapData,
	NpcInfo,
	LanguageHint,
	TextLogEntry,
	UiConfig,
} from '$lib/types';

export const worldState = writable<WorldSnapshot | null>(null);

export const mapData = writable<MapData | null>(null);

export const npcsHere = writable<NpcInfo[]>([]);

export const textLog = writable<TextLogEntry[]>([]);

/** Maximum number of entries kept in the text log before trimming old ones. */
const MAX_TEXT_LOG_SIZE = 500;

/** Trims the text log to MAX_TEXT_LOG_SIZE, removing oldest entries first. */
export function trimTextLog(log: TextLogEntry[]): TextLogEntry[] {
	if (log.length <= MAX_TEXT_LOG_SIZE) return log;
	return log.slice(log.length - MAX_TEXT_LOG_SIZE);
}

export const streamingActive = writable<boolean>(false);

/// Flushes any in-flight streamed NPC line to completion (reveals the full text
/// instantly, cancels the token animation) and clears `streamingActive`.
/// Returns the number of turns flushed. Set by the page controller to the live
/// stream manager's `flushAll`; a no-op until then (#1379). The input field
/// calls this on the player's first keystroke so the current reply snaps fully
/// into view before the next turn is accepted.
export const flushStream = writable<() => number>(() => 0);

/// Current fun loading phrase (e.g. "Consulting the sheep...").
export const loadingPhrase = writable<string>('');

/// Current loading spinner colour as `[R, G, B]`.
function clampChannel(n: unknown): number {
	const x = Math.round(Number(n));
	return Number.isFinite(x) ? Math.max(0, Math.min(255, x)) : 0;
}
function createLoadingColor() {
	const inner = writable<[number, number, number]>([72, 199, 142]);
	return {
		subscribe: inner.subscribe,
		set: (c: [number, number, number]) =>
			inner.set([
				clampChannel(c?.[0]),
				clampChannel(c?.[1]),
				clampChannel(c?.[2]),
			]),
	};
}
export const loadingColor = createLoadingColor();

export const languageHints = writable<LanguageHint[]>([]);

export const nameHints = writable<LanguageHint[]>([]);

export const uiConfig = writable<UiConfig>({
	hints_label: 'Language Hints',
	default_accent: '#b08531',
	splash_text: '',
	active_tile_source: '',
	tile_sources: [],
	auto_pause_timeout_seconds: 300,
	app_icon_url: null,
	favicon_url: null,
});

export const fullMapOpen = writable<boolean>(false);

export const focailOpen = writable<boolean>(false);

/** Monotonically increasing counter incremented each time the player submits
 *  a message. ChatPanel subscribes and scrolls to the bottom unconditionally
 *  when this changes, regardless of current scroll position (#1431 item 4). */
export const playerSubmittedCount = writable<number>(0);

/**
 * Resets focailOpen to false when the viewport transitions to desktop
 * (i.e. when the narrow-viewport media query stops matching).
 *
 * Extracted from the inline matchMedia onChange handler in +page.svelte so
 * that the logic can be unit-tested independently of the component lifecycle.
 * The handler should call this with `e.matches` on every change event.
 *
 * See: fix for #600 — focailOpen must be false on desktop so the Language
 * Hints button doesn't stay in a permanently-pressed-but-invisible state.
 */
export function syncFocailOnViewportChange(matches: boolean): void {
	if (!matches) {
		focailOpen.set(false);
	}
}

/** Maps message ID → Irish word hints for that completed NPC response. */
export const messageHints = writable<Map<string, LanguageHint[]>>(new Map());

/**
 * Drops hint entries whose message is no longer present in `log`.
 *
 * `messageHints` is keyed by message id and was previously only ever added to
 * (`finishNpcStream` calls `messageHints.set` on every NPC turn) with no
 * eviction, so it grew without bound over a long session while the `textLog`
 * it keys into is capped at MAX_TEXT_LOG_SIZE (audit finding H3). Pruning to
 * the live log keeps it bounded by the same cap. Only notifies subscribers
 * when something is actually removed, so it can be wired to every `textLog`
 * change without churning `ChatPanel` reactivity.
 */
export function pruneMessageHints(log: TextLogEntry[]): void {
	const current = get(messageHints);
	if (current.size === 0) return;
	const live = new Set<string>();
	for (const entry of log) {
		if (entry.id) live.add(entry.id);
	}
	let toDelete: string[] | null = null;
	for (const key of current.keys()) {
		if (!live.has(key)) (toDelete ??= []).push(key);
	}
	if (!toDelete) return;
	messageHints.update((map) => {
		for (const key of toDelete) map.delete(key);
		return map;
	});
}

// Keep messageHints bounded to messages still present in the log: every time
// the log changes (including when trimTextLog drops old entries) prune the
// orphaned hint keys. This is the single chokepoint for all trim sites.
textLog.subscribe((log) => pruneMessageHints(log));

/** Last seen `time_label|day_of_week` key for time-rule separators. */
let lastTimeRuleKey: string | null = null;

/** Resets time-rule tracking (new game / branch load / tests). */
export function resetTimeRule(): void {
	lastTimeRuleKey = null;
}

/**
 * Appends a `time-rule` separator entry to the text log when the world
 * clock crosses into a new time-of-day period (or a new day).
 *
 * The game clock runs at 36× real time, but the chronicle had no temporal
 * anchors — this gives the log a quiet "Dusk — Monday" rule between
 * periods. The first snapshot of a session only primes the tracker (no
 * rule), and nothing is emitted while the log is still empty so a rule
 * never precedes the splash.
 */
export function noteTimeRule(snap: WorldSnapshot | null): void {
	// A cleared world state (new game / branch switch / teardown) resets
	// tracking so a stale period key never leaks a spurious separator into
	// the next session's log (PR #1419 review).
	if (!snap) {
		lastTimeRuleKey = null;
		return;
	}
	if (!snap.time_label) return;
	const key = `${snap.time_label}|${snap.day_of_week}`;
	if (lastTimeRuleKey === key) return;
	const isFirst = lastTimeRuleKey === null;
	lastTimeRuleKey = key;
	if (isFirst) return;
	textLog.update((log) => {
		if (log.length === 0) return log;
		return trimTextLog([
			...log,
			{
				source: 'system',
				subtype: 'time-rule',
				content: `${snap.time_label} — ${snap.day_of_week}`,
			},
		]);
	});
}

worldState.subscribe((snap) => noteTimeRule(snap));

/**
 * Appends a user-visible error entry to the text log.
 *
 * Used to surface failures from IPC calls or initial data loads that would
 * otherwise fail silently. The entry is a `system`-sourced message with the
 * `error` subtype so it can be styled distinctly.
 */
export function pushErrorLog(content: string): void {
	textLog.update((log) =>
		trimTextLog([...log, { source: 'system', subtype: 'error', content }]),
	);
}

/** Extracts a concise user-facing string from a thrown IPC error. */
export function formatIpcError(err: unknown): string {
	if (err instanceof Error) return err.message;
	if (typeof err === 'string') return err;
	return 'unknown error';
}

/** Adds a reaction to a message in the text log by message ID. */
export function addReaction(
	messageId: string,
	emoji: string,
	source: string,
): void {
	textLog.update((log) => {
		return log.map((entry) => {
			if (entry.id !== messageId) return entry;
			const reactions = [...(entry.reactions ?? [])];
			if (source === 'player') {
				const existing = reactions.findIndex((r) => r.source === 'player');
				if (existing >= 0) {
					reactions[existing] = { emoji, source };
				} else {
					reactions.push({ emoji, source });
				}
			} else {
				reactions.push({ emoji, source });
			}
			return { ...entry, reactions };
		});
	});
}

/** Removes a matching reaction from a message in the text log.
 *
 * Used to roll back an optimistic `addReaction` when the backend rejects
 * the IPC (#353). Matches on (emoji, source) so a player clicking 😊
 * while an NPC's 😊 already exists doesn't accidentally strip the NPC's
 * reaction on rollback.
 */
export function removeReaction(
	messageId: string,
	emoji: string,
	source: string,
): void {
	textLog.update((log) => {
		return log.map((entry) => {
			if (entry.id !== messageId) return entry;
			const reactions = (entry.reactions ?? []).filter(
				(r) => !(r.emoji === emoji && r.source === source),
			);
			return { ...entry, reactions };
		});
	});
}
