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
});
