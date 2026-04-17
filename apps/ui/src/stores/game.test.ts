import { describe, it, expect, beforeEach } from 'vitest';
import { get } from 'svelte/store';
import { textLog, pushErrorLog, formatIpcError, loadingColor } from './game';

describe('pushErrorLog', () => {
	beforeEach(() => {
		textLog.set([]);
	});

	it('appends a system entry with the error subtype', () => {
		pushErrorLog('Something went wrong');
		const log = get(textLog);
		expect(log.length).toBe(1);
		expect(log[0]).toMatchObject({
			source: 'system',
			subtype: 'error',
			content: 'Something went wrong'
		});
	});

	it('appends to existing log entries rather than replacing them', () => {
		textLog.set([{ source: 'system', content: 'Welcome.' }]);
		pushErrorLog('Network down');
		const log = get(textLog);
		expect(log.length).toBe(2);
		expect(log[0].content).toBe('Welcome.');
		expect(log[1].subtype).toBe('error');
	});
});

describe('loadingColor', () => {
	beforeEach(() => {
		loadingColor.set([72, 199, 142]);
	});

	it('clamps out-of-range values to [0, 255]', () => {
		loadingColor.set([300, -5, 99]);
		expect(get(loadingColor)).toEqual([255, 0, 99]);
	});

	it('clamps non-numeric values to 0', () => {
		loadingColor.set([NaN, 'abc' as any, undefined as any]);
		expect(get(loadingColor)).toEqual([0, 0, 0]);
	});

	it('rounds fractional inputs', () => {
		loadingColor.set([12.7, 200.4, 50]);
		expect(get(loadingColor)).toEqual([13, 200, 50]);
	});
});

describe('formatIpcError', () => {
	it('returns the message from an Error instance', () => {
		expect(formatIpcError(new Error('boom'))).toBe('boom');
	});

	it('returns a string error unchanged', () => {
		expect(formatIpcError('already a string')).toBe('already a string');
	});

	it('falls back to a generic label for unknown shapes', () => {
		expect(formatIpcError({ weird: true })).toBe('unknown error');
		expect(formatIpcError(undefined)).toBe('unknown error');
		expect(formatIpcError(null)).toBe('unknown error');
	});
});
