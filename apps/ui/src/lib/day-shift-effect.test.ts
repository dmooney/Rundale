import { describe, expect, it } from 'vitest';
import { shouldTriggerDayShiftPulse } from './day-shift-effect';

describe('shouldTriggerDayShiftPulse', () => {
	it('does not trigger on first snapshot', () => {
		expect(shouldTriggerDayShiftPulse(null, 7)).toBe(false);
	});

	it('triggers when crossing a boundary hour', () => {
		expect(shouldTriggerDayShiftPulse(6, 7)).toBe(true);
		expect(shouldTriggerDayShiftPulse(11, 12)).toBe(true);
	});

	it('does not trigger when hour changes inside the same phase', () => {
		expect(shouldTriggerDayShiftPulse(8, 9)).toBe(false);
	});

	it('triggers when an update skips over a boundary', () => {
		expect(shouldTriggerDayShiftPulse(4, 8)).toBe(true);
	});

	it('handles overnight wraparound transitions', () => {
		expect(shouldTriggerDayShiftPulse(22, 0)).toBe(true);
	});
});
