import { writable } from 'svelte/store';
import type { WorldSnapshot, MapData, NpcInfo, LanguageHint, TextLogEntry, UiConfig } from '$lib/types';

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
			inner.set([clampChannel(c?.[0]), clampChannel(c?.[1]), clampChannel(c?.[2])]),
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
	auto_pause_timeout_seconds: 300
});

export const fullMapOpen = writable<boolean>(false);

export const focailOpen = writable<boolean>(false);

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
 * Appends a user-visible error entry to the text log.
 *
 * Used to surface failures from IPC calls or initial data loads that would
 * otherwise fail silently. The entry is a `system`-sourced message with the
 * `error` subtype so it can be styled distinctly.
 */
export function pushErrorLog(content: string): void {
	textLog.update((log) =>
		trimTextLog([...log, { source: 'system', subtype: 'error', content }])
	);
}

/** Extracts a concise user-facing string from a thrown IPC error. */
export function formatIpcError(err: unknown): string {
	if (err instanceof Error) return err.message;
	if (typeof err === 'string') return err;
	return 'unknown error';
}

/** Adds a reaction to a message in the text log by message ID. */
export function addReaction(messageId: string, emoji: string, source: string): void {
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
export function removeReaction(messageId: string, emoji: string, source: string): void {
	textLog.update((log) => {
		return log.map((entry) => {
			if (entry.id !== messageId) return entry;
			const reactions = (entry.reactions ?? []).filter(
				(r) => !(r.emoji === emoji && r.source === source)
			);
			return { ...entry, reactions };
		});
	});
}
