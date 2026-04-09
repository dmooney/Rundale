import { writable } from 'svelte/store';
import type { ActiveEffect } from '$lib/effects';
import type { EffectsEngine } from '$lib/effects';

/** Currently active visual effect instances. */
export const activeEffects = writable<ActiveEffect[]>([]);

/** Whether visual effects are enabled (user toggle). */
export const effectsEnabled = writable<boolean>(true);

/** Reference to the running effects engine (for manual triggering via /fx). */
export const effectsEngine = writable<EffectsEngine | null>(null);
