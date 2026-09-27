import { describe, expect, it } from 'vitest';
import { isNotebookLogEntry } from './log';

describe('notebook log filtering', () => {
	it('keeps ordinary journal entries', () => {
		expect(
			isNotebookLogEntry({
				source: 'system',
				subtype: 'location',
				content: 'You are at Kilteevan Village.',
			}),
		).toBe(true);
	});

	it('filters boot/legal text out of the notebook surface', () => {
		expect(
			isNotebookLogEntry({
				source: 'system',
				content: 'Copyright 2026. See LICENSE for details.',
			}),
		).toBe(false);
		expect(
			isNotebookLogEntry({
				source: 'system',
				subtype: 'time-rule',
				content: 'Time passes.',
			}),
		).toBe(false);
	});
});
