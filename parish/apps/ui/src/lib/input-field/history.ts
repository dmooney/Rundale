/**
 * Input history persistence for the contenteditable input field.
 * Uses sessionStorage — not localStorage — because input history may contain
 * sensitive user typing and should be limited to the tab's lifetime.
 */

export const HISTORY_KEY = 'parish-input-history';
export const HISTORY_MAX = 50;

export function loadHistory(): string[] {
	try {
		const raw = sessionStorage.getItem(HISTORY_KEY);
		if (raw) return JSON.parse(raw) as string[];
	} catch {
		/* ignore corrupt data */
	}
	return [];
}

export function saveHistory(h: string[]): void {
	try {
		sessionStorage.setItem(HISTORY_KEY, JSON.stringify(h));
	} catch {
		/* quota */
	}
}
