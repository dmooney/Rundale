const DAY_SHIFT_BOUNDARIES = [5, 7, 12, 17, 20, 23] as const;

/**
 * Returns true when the latest world update crosses a major day-phase boundary.
 *
 * Boundaries are expressed in 24-hour clock local game time and treated as
 * half-open intervals [boundary, nextBoundary).
 */
export function shouldTriggerDayShiftPulse(previousHour: number | null, nextHour: number): boolean {
	if (previousHour === null) return false;
	if (previousHour === nextHour) return false;

	const from = Math.max(0, Math.min(23, Math.floor(previousHour)));
	const to = Math.max(0, Math.min(23, Math.floor(nextHour)));
	const delta = (to - from + 24) % 24;
	if (delta === 0) return false;

	for (const boundary of DAY_SHIFT_BOUNDARIES) {
		const distance = (boundary - from + 24) % 24;
		if (distance > 0 && distance <= delta) {
			return true;
		}
	}

	return false;
}
