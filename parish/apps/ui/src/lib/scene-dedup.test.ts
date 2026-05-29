import { describe, it, expect } from 'vitest';
import { SceneDeduplicator } from './scene-dedup';

describe('SceneDeduplicator', () => {
	it('returns true for the first location (initial state)', () => {
		const dedup = new SceneDeduplicator();
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(true);
	});

	it('returns false when location name is unchanged', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(false);
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(false);
	});

	it('returns true when location name changes', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		expect(dedup.shouldShowDescription('The Crossroads')).toBe(true);
	});

	it('returns true when returning to a previously visited location', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		dedup.shouldShowDescription('The Crossroads');
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(true);
	});

	it('can be reset to initial state', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		dedup.reset();
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(true);
	});

	it('handles rapid location changes', () => {
		const dedup = new SceneDeduplicator();
		expect(dedup.shouldShowDescription('A')).toBe(true);
		expect(dedup.shouldShowDescription('A')).toBe(false);
		expect(dedup.shouldShowDescription('B')).toBe(true);
		expect(dedup.shouldShowDescription('B')).toBe(false);
		expect(dedup.shouldShowDescription('A')).toBe(true);
	});

	it('suppresses the world-update description when a location text-log already showed it (movement)', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan'); // initial scene shown
		// Player moves: the `subtype: "location"` text-log renders the full scene,
		// which calls suppressNextDescription(); the following world-update for the
		// new location must NOT append a second (shorter) scene line.
		dedup.suppressNextDescription();
		expect(dedup.shouldShowDescription('The Crossroads')).toBe(false);
	});

	it('still tracks the location while suppressing, so later idle updates stay deduped', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		dedup.suppressNextDescription();
		expect(dedup.shouldShowDescription('The Crossroads')).toBe(false); // movement: text-log showed it
		expect(dedup.shouldShowDescription('The Crossroads')).toBe(false); // idle: unchanged
	});

	it('only suppresses one world-update, then resumes normal change detection', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		dedup.suppressNextDescription();
		expect(dedup.shouldShowDescription('The Crossroads')).toBe(false); // suppressed move
		// A later location change with no preceding location text-log (e.g. load
		// branch) must still show the scene.
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(true);
	});

	it('does not suppress when no location text-log preceded the world-update (load/restore)', () => {
		const dedup = new SceneDeduplicator();
		dedup.shouldShowDescription('Kilteevan');
		// No suppressNextDescription() call — a load/restore world-update changes
		// the location and should append the scene.
		expect(dedup.shouldShowDescription('The Crossroads')).toBe(true);
	});

	it('clears the suppression flag on reset', () => {
		const dedup = new SceneDeduplicator();
		dedup.suppressNextDescription();
		dedup.reset();
		expect(dedup.shouldShowDescription('Kilteevan')).toBe(true);
	});
});
