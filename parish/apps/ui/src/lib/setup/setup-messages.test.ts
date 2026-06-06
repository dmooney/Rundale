import { describe, it, expect } from 'vitest';
import {
	compactSetupMessages,
	formatSetupStatusMessage,
	sentenceBreakParts,
	isLongSetupWaitMessage,
	SETUP_START_MESSAGE,
	SETUP_HISTORY_LIMIT,
} from './setup-messages';

describe('compactSetupMessages', () => {
	it('filters out blank and whitespace-only entries', () => {
		const result = compactSetupMessages(['hello', '', '  ', 'world']);
		expect(result).toEqual(['hello', 'world']);
	});

	it('keeps the last SETUP_HISTORY_LIMIT messages when over the limit', () => {
		const msgs = Array.from({ length: 100 }, (_, i) => `msg${i}`);
		const result = compactSetupMessages(msgs);
		expect(result).toHaveLength(SETUP_HISTORY_LIMIT);
		expect(result[0]).toBe('msg20');
		expect(result[SETUP_HISTORY_LIMIT - 1]).toBe('msg99');
	});

	it('returns fewer than the limit when input is short', () => {
		const result = compactSetupMessages(['a', 'b', 'c']);
		expect(result).toEqual(['a', 'b', 'c']);
	});

	it('returns an empty array when all messages are blank', () => {
		expect(compactSetupMessages(['', '   ', '\t'])).toEqual([]);
	});
});

describe('formatSetupStatusMessage — literal matches', () => {
	it('maps SETUP_START_MESSAGE to ledger phrase', () => {
		const result = formatSetupStatusMessage(SETUP_START_MESSAGE);
		expect(result).toContain('parish ledger');
	});

	it('maps "pulling manifest" (trimmed) to table-of-contents phrase', () => {
		expect(formatSetupStatusMessage('pulling manifest')).toContain(
			'table of contents',
		);
		// Leading/trailing whitespace is trimmed before matching
		expect(formatSetupStatusMessage('  pulling manifest  ')).toContain(
			'table of contents',
		);
	});

	it('maps "verifying sha256 digest" to wax seals phrase', () => {
		expect(formatSetupStatusMessage('verifying sha256 digest')).toContain(
			'wax seals',
		);
	});

	it('maps "writing manifest" to parish ledger filing phrase', () => {
		expect(formatSetupStatusMessage('writing manifest')).toContain(
			'parish ledger',
		);
	});

	it('maps "success" to parcels phrase', () => {
		expect(formatSetupStatusMessage('success')).toContain('parcels');
	});
});

describe('formatSetupStatusMessage — regex branches', () => {
	it('maps forced-download message including the model name', () => {
		const msg = "Forcing a fresh download of 'llama3:8b'.";
		const result = formatSetupStatusMessage(msg);
		expect(result).toContain('llama3:8b');
		expect(result).toContain('fresh download');
	});

	it('maps clearing message including the model name', () => {
		const msg =
			"Clearing the local copy of 'mistral:7b' before fetching it again...";
		const result = formatSetupStatusMessage(msg);
		expect(result).toContain('mistral:7b');
		expect(result).toContain('old local copy');
	});

	it('maps removed message including the model name', () => {
		const msg = "Local copy of 'phi3:mini' removed. Fetching a fresh copy...";
		const result = formatSetupStatusMessage(msg);
		expect(result).toContain('phi3:mini');
		expect(result).toContain('left the parish');
	});

	it('maps missing-local-copy message including the model name', () => {
		const msg = "No local copy of 'gemma:2b' was present. Fetching it now...";
		const result = formatSetupStatusMessage(msg);
		expect(result).toContain('gemma:2b');
		expect(result).toContain('ledger');
	});

	it('maps fetching-book-of-tales message including the model name', () => {
		const msg = "Fetching the storyteller's book of tales ('qwen:7b')...";
		const result = formatSetupStatusMessage(msg);
		expect(result).toContain('qwen:7b');
		expect(result).toContain('one-time model download');
	});

	it('passes through unrecognized messages unchanged', () => {
		const msg = 'Some arbitrary progress note.';
		expect(formatSetupStatusMessage(msg)).toBe(msg);
	});
});

describe('sentenceBreakParts', () => {
	it('returns a single part with breakAfter false for a single sentence', () => {
		const result = sentenceBreakParts('Hello world.');
		expect(result).toEqual([{ text: 'Hello world.', breakAfter: false }]);
	});

	it('splits on ". " boundaries and marks all but the last with breakAfter true', () => {
		const result = sentenceBreakParts('First. Second. Third.');
		expect(result).toHaveLength(3);
		expect(result[0]).toEqual({ text: 'First.', breakAfter: true });
		expect(result[1]).toEqual({ text: 'Second.', breakAfter: true });
		expect(result[2]).toEqual({ text: 'Third.', breakAfter: false });
	});
});

describe('isLongSetupWaitMessage', () => {
	it('returns true for messages containing download keywords', () => {
		expect(isLongSetupWaitMessage('Pulling manifest for the model')).toBe(true);
		expect(
			isLongSetupWaitMessage("fetching the storyteller's book of tales"),
		).toBe(true);
		expect(isLongSetupWaitMessage('A fresh download is starting')).toBe(true);
		expect(isLongSetupWaitMessage('Fetching a fresh copy now')).toBe(true);
		expect(
			isLongSetupWaitMessage('Removing the local copy before re-fetch'),
		).toBe(true);
		expect(isLongSetupWaitMessage('One-time model download begins')).toBe(true);
	});

	it('returns false for unrelated messages', () => {
		expect(isLongSetupWaitMessage('Starting inference provider setup...')).toBe(
			false,
		);
		expect(isLongSetupWaitMessage('All done.')).toBe(false);
	});
});
