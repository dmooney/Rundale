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

	it('switches to the canonical name only after authoritative introduction', () => {
		const hidden = npc({
			introduced: false,
			name: 'a broad-shouldered smith in a leather apron',
			real_name: 'Seamus Gallagher',
			occupation: 'Blacksmith',
		});
		expect(notebookPersonLabel(hidden)).toBe('Broad-shouldered smith in a');

		const revealed = {
			...hidden,
			name: hidden.real_name,
			introduced: true,
		};
		expect(notebookPersonLabel(revealed)).toBe('Seamus Gallagher');
		expect(notebookPersonInitial(revealed)).toBe('S');
	});
});
