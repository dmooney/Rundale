/**
 * Registry of all visual effect definitions.
 *
 * Each effect is a static definition — the engine evaluates conditions
 * and probability, the EffectsLayer renders the active instances.
 * Individual effect renderers live in components/effects/*.svelte.
 */

import type { EffectDefinition } from './types';

export const EFFECT_DEFINITIONS: EffectDefinition[] = [
	// ── Weather ──────────────────────────────────────────────────────────

	{
		id: 'lightning-flash',
		conditions: { weather: ['Storm'], indoor: false },
		cooldownMs: 15_000,
		intervalMs: [30_000, 90_000],
		durationMs: 800,
		probability: 0.7,
		singleton: true,
	},

	// ── Folklore ─────────────────────────────────────────────────────────

	{
		id: 'bog-lights',
		conditions: {
			indoor: false,
			locationMatch: ['bog', 'marsh', 'moor', 'turf'],
			timeOfDay: ['Dusk', 'Night', 'Midnight'],
		},
		cooldownMs: 120_000,
		intervalMs: [60_000, 180_000],
		durationMs: 45_000,
		probability: 0.5,
		singleton: true,
	},

	{
		id: 'fairy-sprite',
		conditions: {
			indoor: false,
			locationMatch: ['crossroads', 'fairy', 'rath', 'fort', 'bog', 'hawthorn'],
		},
		cooldownMs: 600_000,
		intervalMs: [300_000, 900_000],
		durationMs: 35_000,
		probability: 0.3,
		singleton: true,
	},

	{
		id: 'wind-gust',
		conditions: { weather: ['Storm', 'HeavyRain'], indoor: false },
		cooldownMs: 30_000,
		intervalMs: [20_000, 60_000],
		durationMs: 700,
		probability: 0.5,
		singleton: true,
	},

	// ── Ambient ──────────────────────────────────────────────────────────

	{
		id: 'turf-smoke',
		conditions: {
			indoor: false,
			locationMatch: ['pub', 'darcy', 'cottage', 'farm', 'murphy', 'village', 'shop'],
		},
		cooldownMs: 30_000,
		intervalMs: [20_000, 60_000],
		durationMs: 20_000,
		probability: 0.6,
		singleton: true,
	},

	// ── Weather ──────────────────────────────────────────────────────────

	{
		id: 'fog-creep',
		conditions: { weather: ['Fog'], indoor: false },
		cooldownMs: 0,
		intervalMs: [1_000, 2_000],
		durationMs: 60_000,
		probability: 1.0,
		singleton: true,
	},

	{
		id: 'rain-streaks',
		conditions: { weather: ['LightRain', 'HeavyRain', 'Storm'], indoor: false },
		cooldownMs: 0,
		intervalMs: [1_000, 2_000],
		durationMs: 60_000,
		probability: 1.0,
		singleton: true,
	},
];
