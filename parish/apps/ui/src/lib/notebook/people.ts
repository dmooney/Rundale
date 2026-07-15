import type { NpcInfo } from '$lib/types';

const DESCRIPTION_STOP_WORDS = new Set([
	'with',
	'wearing',
	'wrapped',
	'holding',
]);

function titleCaseFirst(value: string): string {
	return value ? value.slice(0, 1).toUpperCase() + value.slice(1) : value;
}

export function notebookPersonLabel(npc: NpcInfo): string {
	const rawName = (npc.name || npc.real_name || '').trim();
	if (!rawName) return 'Unknown person';
	if (npc.introduced) return rawName;

	const withoutArticle = rawName.replace(/^(an?|the)\s+/i, '').trim();
	const words = withoutArticle.split(/\s+/).filter(Boolean);
	const stopIndex = words.findIndex((word) =>
		DESCRIPTION_STOP_WORDS.has(word.toLowerCase().replace(/[^\w-]/g, '')),
	);
	const usefulWords = words.slice(0, stopIndex === -1 ? 4 : stopIndex);
	const label = usefulWords.length > 0 ? usefulWords.join(' ') : withoutArticle;

	return titleCaseFirst(label.replace(/[,.]+$/, ''));
}

export function notebookPersonInitial(npc: NpcInfo): string {
	return notebookPersonLabel(npc).slice(0, 1).toUpperCase() || '?';
}
