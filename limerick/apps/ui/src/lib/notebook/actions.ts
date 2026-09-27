import type { NpcInfo } from '$lib/types';

export type NotebookAction = 'talk' | 'ask' | 'help' | 'observe' | 'leave';

export function npcAddressName(npc: NpcInfo | null): string | null {
	if (!npc) return null;
	return npc.name || npc.real_name || null;
}

export function notebookActionDraft(
	action: NotebookAction,
	npc: NpcInfo | null,
): string {
	const name = npcAddressName(npc);
	if (!name) {
		switch (action) {
			case 'talk':
				return 'talk to ';
			case 'ask':
				return 'ask about ';
			case 'help':
				return 'offer help';
			case 'observe':
				return 'look around';
			case 'leave':
				return 'leave';
		}
	}
	switch (action) {
		case 'talk':
			return `talk to ${name}`;
		case 'ask':
			return `ask ${name} `;
		case 'help':
			return `offer help to ${name}`;
		case 'observe':
			return `observe ${name}`;
		case 'leave':
			return 'leave';
	}
}
