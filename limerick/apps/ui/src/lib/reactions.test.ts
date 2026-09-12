import { describe, expect, it } from 'vitest';
import { REACTION_PALETTE } from './reactions';
import type { ReactionDef } from './reactions';

describe('REACTION_PALETTE', () => {
	it('has exactly 12 entries', () => {
		expect(REACTION_PALETTE.length).toBe(12);
	});

	it('every entry has emoji, description, and key', () => {
		for (const entry of REACTION_PALETTE) {
			expect(entry).toMatchObject<ReactionDef>({
				emoji: expect.any(String),
				description: expect.any(String),
				key: expect.any(String),
			});
		}
	});

	it('all descriptions are non-empty', () => {
		for (const entry of REACTION_PALETTE) {
			expect(entry.description.length).toBeGreaterThan(0);
		}
	});

	it('all emoji are unique', () => {
		const emojis = REACTION_PALETTE.map((r) => r.emoji);
		expect(new Set(emojis).size).toBe(emojis.length);
	});

	it('all keys are unique', () => {
		const keys = REACTION_PALETTE.map((r) => r.key);
		expect(new Set(keys).size).toBe(keys.length);
	});
});
