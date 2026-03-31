import { writable } from 'svelte/store';
import type { WorldSnapshot, MapData, NpcInfo, LanguageHint, TextLogEntry, UiConfig } from '$lib/types';

export const worldState = writable<WorldSnapshot | null>(null);

export const mapData = writable<MapData | null>(null);

export const npcsHere = writable<NpcInfo[]>([]);

export const textLog = writable<TextLogEntry[]>([]);

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
