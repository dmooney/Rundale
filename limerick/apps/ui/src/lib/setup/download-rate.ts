export const RATE_SAMPLE_WINDOW_MS = 30_000;
export const RATE_UPDATE_INTERVAL_MS = 750;
export const MIN_RATE_SAMPLE_SPAN_MS = 750;
export const RATE_NEW_SAMPLE_WEIGHT = 0.15;

export function formatBytes(bytes: number): string {
	const units = ['B', 'KB', 'MB', 'GB'];
	let value = Math.max(0, bytes);
	let unitIndex = 0;
	while (value >= 1024 && unitIndex < units.length - 1) {
		value /= 1024;
		unitIndex += 1;
	}
	const digits = value >= 100 || unitIndex === 0 ? 0 : value >= 10 ? 1 : 2;
	return `${value.toFixed(digits)} ${units[unitIndex]}`;
}

export function formatDuration(seconds: number): string {
	const totalSeconds = Math.max(0, Math.round(seconds));
	const mins = Math.floor(totalSeconds / 60);
	const secs = totalSeconds % 60;
	if (mins >= 60) {
		const hours = Math.floor(mins / 60);
		const restMins = mins % 60;
		return `${hours}h ${restMins}m`;
	}
	return `${mins}:${secs.toString().padStart(2, '0')}`;
}

export function formatElapsed(seconds: number): string {
	const mins = Math.floor(seconds / 60);
	const secs = seconds % 60;
	return `${mins}:${secs.toString().padStart(2, '0')}`;
}

export function formatDownloadStats(
	downloadCompleted: number,
	downloadTotal: number,
	downloadSpeedBps: number | null,
	downloadEtaSeconds: number | null,
): string {
	if (downloadTotal <= 0) return '';

	const parts = [
		`${formatBytes(downloadCompleted)} of ${formatBytes(downloadTotal)}`,
	];
	if (downloadSpeedBps !== null && downloadSpeedBps > 0) {
		parts.push(`${formatBytes(downloadSpeedBps)}/s`);
	}
	if (downloadEtaSeconds !== null) {
		parts.push(`${formatDuration(downloadEtaSeconds)} left`);
	}
	return parts.join(' \u2022 ');
}

/** The displayed download rate + ETA after recording a progress sample. */
export interface DownloadRate {
	/** Smoothed bytes-per-second, or `null` until enough samples accrue. */
	speedBps: number | null;
	/** Estimated seconds remaining, or `null` when not computable. */
	etaSeconds: number | null;
}

/**
 * Sliding-window + EMA bytes/sec estimator for the setup download progress
 * (#1200 TD-044).
 *
 * Extracted verbatim from `SetupOverlay.svelte`, which previously held the
 * `progressSamples` / `lastRateUpdateAt` / `speedBps` / `etaSeconds` rate state
 * and the smoothing math inline. The component now keeps only the rendered
 * `$state` mirror of [`DownloadRate`]; all the sampling logic \u2014 pure, no Svelte
 * reactivity \u2014 lives here and is unit-testable in isolation.
 */
export class DownloadRateTracker {
	private speedBps: number | null = null;
	private etaSeconds: number | null = null;
	private lastRateUpdateAt = 0;
	private progressSamples: Array<{ completed: number; timestamp: number }> = [];

	/** The current displayed rate + ETA. */
	get rate(): DownloadRate {
		return { speedBps: this.speedBps, etaSeconds: this.etaSeconds };
	}

	/**
	 * Clears all rate state. When `initialCompleted` is given, seeds a single
	 * baseline sample (used when a fresh transfer's first byte count arrives) so
	 * the next sample can compute a span; otherwise the window starts empty.
	 */
	reset(initialCompleted?: number): DownloadRate {
		this.speedBps = null;
		this.etaSeconds = null;
		this.lastRateUpdateAt = 0;
		this.progressSamples =
			initialCompleted === undefined
				? []
				: [{ completed: initialCompleted, timestamp: performance.now() }];
		return this.rate;
	}

	private updateEtaFromDisplayedSpeed(completed: number, total: number): void {
		if (this.speedBps === null || this.speedBps <= 0 || total <= completed) {
			this.etaSeconds = null;
			return;
		}
		this.etaSeconds = (total - completed) / this.speedBps;
	}

	/** Records a `(completed, total)` sample and recomputes the rate + ETA. */
	record(completed: number, total: number): DownloadRate {
		const now = performance.now();
		this.progressSamples = [
			...this.progressSamples.filter(
				(sample) => sample.timestamp >= now - RATE_SAMPLE_WINDOW_MS,
			),
			{ completed, timestamp: now },
		];

		const first = this.progressSamples[0];
		const last = this.progressSamples.at(-1);
		if (!first || !last || last.completed <= first.completed) {
			this.updateEtaFromDisplayedSpeed(completed, total);
			return this.rate;
		}

		const spanMs = last.timestamp - first.timestamp;
		if (spanMs < MIN_RATE_SAMPLE_SPAN_MS) {
			this.updateEtaFromDisplayedSpeed(completed, total);
			return this.rate;
		}
		if (
			this.lastRateUpdateAt > 0 &&
			now - this.lastRateUpdateAt < RATE_UPDATE_INTERVAL_MS
		) {
			this.updateEtaFromDisplayedSpeed(completed, total);
			return this.rate;
		}

		const windowBps = (last.completed - first.completed) / (spanMs / 1000);
		this.speedBps =
			this.speedBps === null
				? windowBps
				: this.speedBps * (1 - RATE_NEW_SAMPLE_WEIGHT) +
					windowBps * RATE_NEW_SAMPLE_WEIGHT;

		this.updateEtaFromDisplayedSpeed(completed, total);
		this.lastRateUpdateAt = now;
		return this.rate;
	}
}
