import { describe, expect, it } from 'vitest';
import { notebookActionDraft, npcAddressName } from './actions';
import type { NpcInfo } from '$lib/types';

const roisin: NpcInfo = {
	npc_id: 6,
	name: 'Roisin Connolly',
	real_name: 'Roisin Connolly',
	occupation: 'shopkeeper',
	mood: 'wary',
	introduced: true,
	mood_emoji: '•',
};

describe('notebook actions', () => {
	it('uses the visible NPC name for action drafts', () => {
		expect(npcAddressName(roisin)).toBe('Roisin Connolly');
		expect(notebookActionDraft('talk', roisin)).toBe('talk to Roisin Connolly');
		expect(notebookActionDraft('ask', roisin)).toBe('ask Roisin Connolly ');
		expect(notebookActionDraft('help', roisin)).toBe(
			'offer help to Roisin Connolly',
		);
		expect(notebookActionDraft('observe', roisin)).toBe(
			'observe Roisin Connolly',
		);
		expect(notebookActionDraft('leave', roisin)).toBe('leave');
	});

	it('falls back to place-level starters when no person is selected', () => {
		expect(notebookActionDraft('talk', null)).toBe('talk to ');
		expect(notebookActionDraft('ask', null)).toBe('ask about ');
		expect(notebookActionDraft('help', null)).toBe('offer help');
		expect(notebookActionDraft('observe', null)).toBe('look around');
		expect(notebookActionDraft('leave', null)).toBe('leave');
	});
});
