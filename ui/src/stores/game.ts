import { writable } from 'svelte/store';
import type { WorldSnapshot, MapData, NpcInfo, IrishWordHint, TextLogEntry } from '$lib/types';

export const worldState = writable<WorldSnapshot | null>(null);

export const mapData = writable<MapData | null>(null);

export const npcsHere = writable<NpcInfo[]>([]);

export const textLog = writable<TextLogEntry[]>([]);

export const streamingActive = writable<boolean>(false);

export const irishHints = writable<IrishWordHint[]>([]);
