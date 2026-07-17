import { writable } from 'svelte/store';
import type { BugContext } from '$lib/types';

/** Whether the bug-report modal is visible. */
export const bugReportVisible = writable<boolean>(false);

/**
 * Optional debug-panel record the modal was opened from. `null` when the
 * report is filed from the toolbar with no specific record attached.
 */
export const bugReportContext = writable<BugContext | null>(null);

/**
 * Screenshot (`data:image/png;base64,…`) captured the moment the modal was
 * opened, so it shows the game view the user was looking at rather than the
 * bug-report modal itself. `null` if capture failed.
 */
export const bugReportScreenshot = writable<string | null>(null);

export interface PreparedBugReport {
	context: BugContext | null;
	screenshot: string | null;
}

/**
 * Captures the report payload without publishing it to the shared modal
 * stores. Coordinators can discard a stale capture without disturbing a
 * newer report that already owns the modal.
 */
export async function prepareBugReport(
	context?: BugContext,
): Promise<PreparedBugReport> {
	let screenshot: string | null = null;
	try {
		const { captureScreen } = await import('$lib/screenshot');
		screenshot = await captureScreen();
	} catch {
		// A failed capture is non-fatal; the report still opens without an image.
	}
	return { context: context ?? null, screenshot };
}

/** Publishes one fully captured report to the modal stores atomically. */
export function showPreparedBugReport(report: PreparedBugReport): void {
	bugReportContext.set(report.context);
	bugReportScreenshot.set(report.screenshot);
	bugReportVisible.set(true);
}

/**
 * Opens the bug-report modal, optionally pre-filled with a debug record.
 *
 * Captures the screenshot *before* showing the modal so the image reflects the
 * game (or open debug panel) rather than the modal overlay. A failed capture
 * is non-fatal — the report is still filed without an image.
 */
export async function openBugReport(context?: BugContext): Promise<void> {
	showPreparedBugReport(await prepareBugReport(context));
}

/** Closes the bug-report modal and clears any attached context/screenshot. */
export function closeBugReport(): void {
	bugReportVisible.set(false);
	bugReportContext.set(null);
	bugReportScreenshot.set(null);
}
