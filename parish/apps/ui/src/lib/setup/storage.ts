import {
	formatSetupStatusMessage,
	compactSetupMessages,
	INITIAL_SETUP_MESSAGE,
} from './setup-messages';

export const SETUP_COMPLETE_SESSION_KEY = 'rundale-setup-complete';
export const SETUP_ACTIVITY_SESSION_KEY = 'rundale-setup-activity';

export type StoredSetupActivity = {
	currentPhrase: string;
	messages: string[];
	completed: number;
	total: number;
};

export function readSetupCompleteFlag(): boolean {
	try {
		return sessionStorage.getItem(SETUP_COMPLETE_SESSION_KEY) === 'true';
	} catch {
		return false;
	}
}

export function readSetupActivity(): StoredSetupActivity | null {
	try {
		const raw = sessionStorage.getItem(SETUP_ACTIVITY_SESSION_KEY);
		if (!raw) return null;
		const parsed = JSON.parse(raw) as Partial<StoredSetupActivity>;
		const storedMessages = Array.isArray(parsed.messages)
			? parsed.messages.filter((msg): msg is string => typeof msg === 'string')
			: [];
		const displayMessages = storedMessages.map(formatSetupStatusMessage);
		return {
			currentPhrase:
				typeof parsed.currentPhrase === 'string'
					? formatSetupStatusMessage(parsed.currentPhrase)
					: (displayMessages.at(-1) ?? INITIAL_SETUP_MESSAGE),
			messages: compactSetupMessages(displayMessages),
			completed: typeof parsed.completed === 'number' ? parsed.completed : 0,
			total: typeof parsed.total === 'number' ? parsed.total : 0,
		};
	} catch {
		return null;
	}
}

export function markSetupComplete(): void {
	try {
		sessionStorage.setItem(SETUP_COMPLETE_SESSION_KEY, 'true');
	} catch {
		// Ignore storage failures; the backend snapshot is still authoritative.
	}
	clearSetupActivity();
}

export function clearSetupComplete(): void {
	try {
		sessionStorage.removeItem(SETUP_COMPLETE_SESSION_KEY);
	} catch {
		// Ignore storage failures; this only affects remount flash suppression.
	}
}

export function persistSetupActivity(
	currentPhrase: string,
	messages: string[],
	completed: number,
	total: number,
): void {
	try {
		sessionStorage.setItem(
			SETUP_ACTIVITY_SESSION_KEY,
			JSON.stringify({ currentPhrase, messages, completed, total }),
		);
	} catch {
		// Ignore storage failures; this only preserves activity across remounts.
	}
}

export function clearSetupActivity(): void {
	try {
		sessionStorage.removeItem(SETUP_ACTIVITY_SESSION_KEY);
	} catch {
		// Ignore storage failures.
	}
}
