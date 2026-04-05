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

/// Current loading spinner character (e.g. "✛").
export const loadingSpinner = writable<string>('');

/// Current fun loading phrase (e.g. "Consulting the sheep...").
export const loadingPhrase = writable<string>('');

/// Current loading spinner colour as `[R, G, B]`.
export const loadingColor = writable<[number, number, number]>([72, 199, 142]);

export const languageHints = writable<LanguageHint[]>([]);

export const nameHints = writable<LanguageHint[]>([]);

export const uiConfig = writable<UiConfig>({
	hints_label: 'Language Hints',
	default_accent: '#c4a35a',
	splash_text: ''
});

export const fullMapOpen = writable<boolean>(false);

/** Adds a reaction to a message in the text log by message ID. */
export function addReaction(messageId: string, emoji: string, source: string): void {
	textLog.update((log) => {
		const entry = log.find((e) => e.id === messageId);
		if (!entry) return log;

		const reactions = entry.reactions ?? [];
		// Player: one reaction per message (replace existing)
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
		entry.reactions = reactions;
		return [...log];
	});
}
