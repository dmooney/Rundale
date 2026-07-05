import type { TextLogEntry } from '$lib/types';

const NON_NOTEBOOK_PATTERNS = [
	/copyright/i,
	/license/i,
	/gpl/i,
	/gnu general public license/i,
];

export function isNotebookLogEntry(entry: TextLogEntry): boolean {
	const content = entry.content.trim();
	if (!content) return false;
	if (entry.subtype === 'time-rule') return false;
	return !NON_NOTEBOOK_PATTERNS.some((pattern) => pattern.test(content));
}
