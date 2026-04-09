import { describe, expect, it } from 'vitest';
import { getStreamChunkDelayMs, takeNextStreamChunk } from './stream-pacing';

describe('takeNextStreamChunk', () => {
	it('emits one complete word at a time with trailing whitespace', () => {
		expect(takeNextStreamChunk('I went to the market')).toEqual({
			chunk: 'I ',
			rest: 'went to the market'
		});
	});

	it('waits for a word boundary before emitting mid-stream text', () => {
		expect(takeNextStreamChunk('Dia')).toEqual({
			chunk: null,
			rest: 'Dia'
		});
		expect(takeNextStreamChunk(' Dia')).toEqual({
			chunk: null,
			rest: ' Dia'
		});
	});

	it('keeps leading whitespace attached to the next word', () => {
		expect(takeNextStreamChunk('\n\nDia dhuit')).toEqual({
			chunk: '\n\nDia ',
			rest: 'dhuit'
		});
	});

	it('flushes the final word when the stream ends', () => {
		expect(takeNextStreamChunk('slan', true)).toEqual({
			chunk: 'slan',
			rest: ''
		});
	});

	it('flushes whitespace-only tails at the end of a stream', () => {
		expect(takeNextStreamChunk('\n\n', true)).toEqual({
			chunk: '\n\n',
			rest: ''
		});
	});
});

describe('getStreamChunkDelayMs', () => {
	it('uses the same base pacing for single words', () => {
		expect(getStreamChunkDelayMs('well ')).toBe(120);
	});

	it('adds a clause pause after commas', () => {
		expect(getStreamChunkDelayMs('well, ')).toBeGreaterThan(getStreamChunkDelayMs('well '));
	});

	it('adds a larger pause after sentence endings', () => {
		expect(getStreamChunkDelayMs('indeed. ')).toBeGreaterThan(getStreamChunkDelayMs('indeed, '));
	});
});
