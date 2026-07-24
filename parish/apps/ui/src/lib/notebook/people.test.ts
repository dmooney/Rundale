import { describe, expect, it } from 'vitest';
import type { NpcInfo } from '$lib/types';
import { notebookPersonInitial, notebookPersonLabel } from './people';

function npc(overrides: Partial<NpcInfo>): NpcInfo {
	return {
		npc_id: 6,
		name: 'Roisin Connolly',
		real_name: 'Roisin Connolly',
		occupation: '',
		mood: 'wary',
		introduced: true,
		mood_emoji: '•',
		...overrides,
	};
}

describe('notebook people labels', () => {
	it('keeps introduced names intact', () => {
		expect(notebookPersonLabel(npc({ name: 'Roisin Connolly' }))).toBe(
			'Roisin Connolly',
		);
	});

	it('turns unintroduced descriptions into compact margin labels', () => {
		const person = npc({
			introduced: false,
			name: 'a lean, red-haired young man with hard eyes',
		});

		expect(notebookPersonLabel(person)).toBe('Lean, red-haired young man');
		expect(notebookPersonInitial(person)).toBe('L');
	});
});
