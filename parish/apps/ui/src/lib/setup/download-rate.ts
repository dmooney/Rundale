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
	downloadEtaSeconds: number | null
): string {
	if (downloadTotal <= 0) return '';

	const parts = [`${formatBytes(downloadCompleted)} of ${formatBytes(downloadTotal)}`];
	if (downloadSpeedBps !== null && downloadSpeedBps > 0) {
		parts.push(`${formatBytes(downloadSpeedBps)}/s`);
	}
	if (downloadEtaSeconds !== null) {
		parts.push(`${formatDuration(downloadEtaSeconds)} left`);
	}
	return parts.join(' \u2022 ');
}
