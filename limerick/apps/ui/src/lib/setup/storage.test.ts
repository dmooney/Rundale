import { beforeEach, describe, it, expect, vi } from 'vitest';
import {
	readSetupCompleteFlag,
	readSetupActivity,
	markSetupComplete,
	clearSetupComplete,
	persistSetupActivity,
	clearSetupActivity,
	SETUP_COMPLETE_SESSION_KEY,
	SETUP_ACTIVITY_SESSION_KEY,
} from './storage';
import { INITIAL_SETUP_MESSAGE } from './setup-messages';

beforeEach(() => {
	sessionStorage.clear();
});

// ── readSetupCompleteFlag ────────────────────────────────────────────────────

describe('readSetupCompleteFlag', () => {
	it('returns false when the key is absent', () => {
		expect(readSetupCompleteFlag()).toBe(false);
	});

	it('returns true when the key is "true"', () => {
		sessionStorage.setItem(SETUP_COMPLETE_SESSION_KEY, 'true');
		expect(readSetupCompleteFlag()).toBe(true);
	});

	it('returns false for any value other than "true"', () => {
		sessionStorage.setItem(SETUP_COMPLETE_SESSION_KEY, '1');
		expect(readSetupCompleteFlag()).toBe(false);
	});

	it('returns false when sessionStorage.getItem throws', () => {
		vi.spyOn(sessionStorage, 'getItem').mockImplementationOnce(() => {
			throw new Error('storage unavailable');
		});
		expect(readSetupCompleteFlag()).toBe(false);
	});
});

// ── readSetupActivity ────────────────────────────────────────────────────────

describe('readSetupActivity', () => {
	it('returns null when the key is absent', () => {
		expect(readSetupActivity()).toBeNull();
	});

	it('returns null when the stored value is corrupt JSON', () => {
		sessionStorage.setItem(SETUP_ACTIVITY_SESSION_KEY, '{bad json}');
		expect(readSetupActivity()).toBeNull();
	});

	it('returns null when sessionStorage.getItem throws', () => {
		vi.spyOn(sessionStorage, 'getItem').mockImplementationOnce(() => {
			throw new Error('storage unavailable');
		});
		expect(readSetupActivity()).toBeNull();
	});

	it('parses a valid activity and transforms messages through formatSetupStatusMessage', () => {
		const raw = {
			currentPhrase: 'pulling manifest',
			messages: ['pulling manifest', 'success'],
			completed: 512,
			total: 1024,
		};
		sessionStorage.setItem(SETUP_ACTIVITY_SESSION_KEY, JSON.stringify(raw));
		const result = readSetupActivity();
		expect(result).not.toBeNull();
		// formatSetupStatusMessage maps "pulling manifest" to a different string
		expect(result!.currentPhrase).not.toBe('pulling manifest');
		expect(result!.currentPhrase).toContain('table of contents');
		expect(result!.messages[1]).toContain('parcels'); // "success" -> parcels phrase
		expect(result!.completed).toBe(512);
		expect(result!.total).toBe(1024);
	});

	it('falls back to INITIAL_SETUP_MESSAGE when currentPhrase is missing', () => {
		const raw = { messages: [], completed: 0, total: 0 };
		sessionStorage.setItem(SETUP_ACTIVITY_SESSION_KEY, JSON.stringify(raw));
		const result = readSetupActivity();
		expect(result!.currentPhrase).toBe(INITIAL_SETUP_MESSAGE);
	});

	it('falls back to last formatted message when currentPhrase absent but messages present', () => {
		const raw = { messages: ['success'], completed: 0, total: 0 };
		sessionStorage.setItem(SETUP_ACTIVITY_SESSION_KEY, JSON.stringify(raw));
		const result = readSetupActivity();
		// last message "success" is formatted to the parcels phrase
		expect(result!.currentPhrase).toContain('parcels');
	});

	it('ignores non-string entries in the messages array', () => {
		const raw = {
			currentPhrase: 'success',
			messages: ['success', 42, null, 'pulling manifest'],
			completed: 0,
			total: 0,
		};
		sessionStorage.setItem(SETUP_ACTIVITY_SESSION_KEY, JSON.stringify(raw));
		const result = readSetupActivity();
		// Only two string messages survive the filter
		expect(result!.messages).toHaveLength(2);
	});

	it('defaults completed and total to 0 when absent', () => {
		const raw = { currentPhrase: 'success', messages: [] };
		sessionStorage.setItem(SETUP_ACTIVITY_SESSION_KEY, JSON.stringify(raw));
		const result = readSetupActivity();
		expect(result!.completed).toBe(0);
		expect(result!.total).toBe(0);
	});
});

// ── persistSetupActivity ─────────────────────────────────────────────────────

describe('persistSetupActivity', () => {
	it('stores serialized activity in sessionStorage', () => {
		persistSetupActivity('pulling manifest', ['pulling manifest'], 100, 200);
		const raw = sessionStorage.getItem(SETUP_ACTIVITY_SESSION_KEY);
		expect(raw).not.toBeNull();
		const parsed = JSON.parse(raw!);
		expect(parsed.currentPhrase).toBe('pulling manifest');
		expect(parsed.completed).toBe(100);
		expect(parsed.total).toBe(200);
	});

	it('does not throw when sessionStorage.setItem throws', () => {
		vi.spyOn(sessionStorage, 'setItem').mockImplementationOnce(() => {
			throw new DOMException('QuotaExceededError');
		});
		expect(() => persistSetupActivity('msg', [], 0, 0)).not.toThrow();
	});
});

// ── markSetupComplete ────────────────────────────────────────────────────────

describe('markSetupComplete', () => {
	it('sets the complete flag and clears the activity key', () => {
		persistSetupActivity('msg', ['msg'], 0, 0);
		markSetupComplete();
		expect(sessionStorage.getItem(SETUP_COMPLETE_SESSION_KEY)).toBe('true');
		expect(sessionStorage.getItem(SETUP_ACTIVITY_SESSION_KEY)).toBeNull();
	});
});

// ── clearSetupComplete ───────────────────────────────────────────────────────

describe('clearSetupComplete', () => {
	it('removes the complete flag', () => {
		sessionStorage.setItem(SETUP_COMPLETE_SESSION_KEY, 'true');
		clearSetupComplete();
		expect(sessionStorage.getItem(SETUP_COMPLETE_SESSION_KEY)).toBeNull();
	});

	it('does not throw when sessionStorage.removeItem throws', () => {
		vi.spyOn(sessionStorage, 'removeItem').mockImplementationOnce(() => {
			throw new Error('storage unavailable');
		});
		expect(() => clearSetupComplete()).not.toThrow();
	});
});

// ── clearSetupActivity ───────────────────────────────────────────────────────

describe('clearSetupActivity', () => {
	it('removes the activity key', () => {
		persistSetupActivity('msg', ['msg'], 0, 0);
		clearSetupActivity();
		expect(sessionStorage.getItem(SETUP_ACTIVITY_SESSION_KEY)).toBeNull();
	});

	it('does not throw when sessionStorage.removeItem throws', () => {
		vi.spyOn(sessionStorage, 'removeItem').mockImplementationOnce(() => {
			throw new Error('storage unavailable');
		});
		expect(() => clearSetupActivity()).not.toThrow();
	});
});
