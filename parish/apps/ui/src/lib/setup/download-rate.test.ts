import { describe, it, expect, vi, afterEach } from 'vitest';
import {
	formatBytes,
	formatDuration,
	formatElapsed,
	formatDownloadStats,
	DownloadRateTracker,
} from './download-rate';

describe('formatBytes', () => {
	it('returns "0 B" for zero', () => {
		expect(formatBytes(0)).toBe('0 B');
	});

	it('clamps negative input to 0 B', () => {
		expect(formatBytes(-100)).toBe('0 B');
	});

	it('renders bytes below 1024 without a decimal', () => {
		expect(formatBytes(1)).toBe('1 B');
		expect(formatBytes(1023)).toBe('1023 B');
	});

	it('converts 1024 to 1.00 KB', () => {
		expect(formatBytes(1024)).toBe('1.00 KB');
	});

	it('renders KB with one decimal for values 10-99.x KB', () => {
		expect(formatBytes(10 * 1024)).toBe('10.0 KB');
	});

	it('renders KB with no decimal for values >= 100 KB', () => {
		expect(formatBytes(100 * 1024)).toBe('100 KB');
	});

	it('converts 1024*1024 to 1.00 MB', () => {
		expect(formatBytes(1024 * 1024)).toBe('1.00 MB');
	});

	it('renders MB with one decimal for 10–99.x MB', () => {
		expect(formatBytes(10 * 1024 * 1024)).toBe('10.0 MB');
	});

	it('converts 1 GB correctly', () => {
		expect(formatBytes(1024 * 1024 * 1024)).toBe('1.00 GB');
	});

	it('does not go beyond GB', () => {
		// 2 TB stays in GB notation
		const twoTB = 2 * 1024 * 1024 * 1024 * 1024;
		expect(formatBytes(twoTB)).toMatch(/GB$/);
	});
});

describe('formatDuration', () => {
	it('returns "0:00" for zero', () => {
		expect(formatDuration(0)).toBe('0:00');
	});

	it('clamps negative to 0:00', () => {
		expect(formatDuration(-5)).toBe('0:00');
	});

	it('formats under one minute', () => {
		expect(formatDuration(59)).toBe('0:59');
	});

	it('formats exactly one minute', () => {
		expect(formatDuration(60)).toBe('1:00');
	});

	it('pads seconds to two digits', () => {
		expect(formatDuration(61)).toBe('1:01');
	});

	it('formats 59m59s', () => {
		expect(formatDuration(59 * 60 + 59)).toBe('59:59');
	});

	it('formats >= 1 hour in h m notation', () => {
		expect(formatDuration(3600)).toBe('1h 0m');
		expect(formatDuration(3661)).toBe('1h 1m');
		expect(formatDuration(7200)).toBe('2h 0m');
		expect(formatDuration(5400)).toBe('1h 30m');
	});

	it('rounds fractional seconds', () => {
		expect(formatDuration(59.6)).toBe('1:00');
	});
});

describe('formatElapsed', () => {
	it('formats elapsed time without clamping', () => {
		expect(formatElapsed(65)).toBe('1:05');
		expect(formatElapsed(0)).toBe('0:00');
	});
});

describe('formatDownloadStats', () => {
	it('returns empty string when total is zero or negative', () => {
		expect(formatDownloadStats(0, 0, null, null)).toBe('');
		expect(formatDownloadStats(0, -1, null, null)).toBe('');
	});

	it('shows completed/total without speed or eta', () => {
		const result = formatDownloadStats(512, 1024, null, null);
		expect(result).toBe('512 B of 1.00 KB');
	});

	it('appends speed when non-null and positive', () => {
		const result = formatDownloadStats(512, 1024, 256, null);
		expect(result).toContain('256 B/s');
	});

	it('omits speed when zero', () => {
		const result = formatDownloadStats(512, 1024, 0, null);
		expect(result).not.toContain('/s');
	});

	it('appends ETA when non-null', () => {
		const result = formatDownloadStats(512, 1024, null, 30);
		expect(result).toContain('0:30 left');
	});

	it('includes all three parts when all provided', () => {
		const result = formatDownloadStats(512, 1024, 256, 30);
		expect(result).toContain('512 B of 1.00 KB');
		expect(result).toContain('256 B/s');
		expect(result).toContain('0:30 left');
		// Joined with bullet separator
		expect(result).toContain(' • ');
	});
});

describe('DownloadRateTracker', () => {
	afterEach(() => {
		vi.restoreAllMocks();
	});

	it('reset() with no argument clears speed and ETA', () => {
		const tracker = new DownloadRateTracker();
		expect(tracker.reset()).toEqual({ speedBps: null, etaSeconds: null });
		expect(tracker.rate).toEqual({ speedBps: null, etaSeconds: null });
	});

	it('reports no rate until two samples span the minimum window', () => {
		// Single seeded baseline sample → no span yet → speed stays null.
		let now = 1_000;
		vi.spyOn(performance, 'now').mockImplementation(() => now);
		const tracker = new DownloadRateTracker();
		tracker.reset(0);
		now += 100; // below MIN_RATE_SAMPLE_SPAN_MS (750)
		expect(tracker.record(50, 1000)).toEqual({
			speedBps: null,
			etaSeconds: null,
		});
	});

	it('computes a smoothed bytes/sec and ETA across a sufficient span', () => {
		let now = 0;
		vi.spyOn(performance, 'now').mockImplementation(() => now);
		const tracker = new DownloadRateTracker();
		tracker.reset(0); // baseline at t=0, completed=0
		now = 1000; // 1s later, past the 750ms min span + update interval
		const { speedBps, etaSeconds } = tracker.record(1000, 4000);
		// 1000 bytes over 1s → 1000 B/s (first sample is unsmoothed).
		expect(speedBps).toBeCloseTo(1000, 5);
		// 3000 bytes remaining at 1000 B/s → 3s.
		expect(etaSeconds).toBeCloseTo(3, 5);
	});

	it('clears ETA once completed reaches total', () => {
		let now = 0;
		vi.spyOn(performance, 'now').mockImplementation(() => now);
		const tracker = new DownloadRateTracker();
		tracker.reset(0);
		now = 1000;
		const { etaSeconds } = tracker.record(4000, 4000);
		expect(etaSeconds).toBeNull();
	});
});
