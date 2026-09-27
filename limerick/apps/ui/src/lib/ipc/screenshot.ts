/**
 * Screenshot capture/persistence, bug reporting, and external-URL opening
 * IPC (#1200 TD-054).
 */

import type { BugContext, BugReportResult } from '../types';
import { command, onEvent, IS_TAURI } from './transport';

export interface ScreenshotInfo {
	/** Absolute filesystem path to the PNG written by the backend. */
	path: string;
	/** ISO-8601 UTC timestamp the file was written (`YYYY-MM-DDTHH:MM:SSZ`). */
	taken_at: string;
	/** Size of the PNG payload in bytes. */
	size_bytes: number;
}

export interface GraphicalReadiness {
	launch_token: string;
	ready: boolean;
}

/** Reads the per-process token used to bind UI readiness to this Tauri launch. */
export const getGraphicalReadiness = () =>
	command<GraphicalReadiness>('get_graphical_readiness');

/** Reports that the mounted graphical surface can receive screenshot requests. */
export const reportGraphicalReady = (launchToken: string) =>
	command<void>('report_graphical_ready', { launchToken });

/** Reports why the mounted renderer could not produce a graphical frame. */
export const reportGraphicalError = (launchToken: string, error: string) =>
	command<void>('report_graphical_error', { launchToken, error });

/** Invalidates graphical capture readiness when the mounted surface is removed. */
export const reportGraphicalUnready = (launchToken: string) =>
	command<void>('report_graphical_unready', { launchToken });

/**
 * Persists a screenshot captured by `captureScreen()` (in `lib/screenshot.ts`).
 *
 * `dataUrl` must be a `data:image/png;base64,...` string. Tauri-only: the
 * web server returns 501 since the headless backend has no DOM to capture.
 */
export const saveScreenshot = (dataUrl: string) =>
	command<ScreenshotInfo>('save_screenshot', { dataUrl });

/**
 * Files a bug report — bundles a screenshot, recent logs, and current game
 * state into a GitHub issue (or an on-disk bundle in dry-run / no-token mode).
 *
 * Keys are camelCase so the single `command()` adapter works across both
 * transports: Tauri maps `screenshotDataUrl` → the `screenshot_data_url`
 * argument, and the web route's `BugReportRequest` uses `rename_all =
 * "camelCase"`.
 */
export const submitBugReport = (args: {
	title: string;
	description: string;
	screenshotDataUrl?: string;
	context?: BugContext;
}) => command<BugReportResult>('submit_bug_report', args);

/**
 * Opens a URL in the system's default browser.
 *
 * In Tauri mode this invokes the `open_url` backend command, which uses the OS
 * process spawner (`open` on macOS, `start` on Windows, `xdg-open` on Linux)
 * because Tauri v2 blocks `<a target="_blank">` external navigation by default
 * (#1223). In web mode it falls through to a plain `window.open` call.
 *
 * Only `https://` and `http://` URLs are accepted; the backend rejects others.
 */
export async function openUrl(url: string): Promise<void> {
	if (IS_TAURI) {
		await command<void>('open_url', { url });
	} else {
		window.open(url, '_blank', 'noopener,noreferrer');
	}
}

/**
 * Reads metadata for the most recently captured screenshot, or `null` if
 * none has been captured this session (or the cached file was deleted).
 */
export const getLatestScreenshot = () =>
	command<ScreenshotInfo | null>('get_latest_screenshot');

/**
 * Sends the result of an agent-triggered screenshot back to the MCP bridge.
 *
 * Called by the frontend after it receives a `request-screenshot` event
 * (via `onRequestScreenshot`) and completes the capture. The bridge handler
 * that emitted the event is waiting on a oneshot channel keyed by
 * `request_id`; this call unblocks it so it can return `ScreenshotInfo` to
 * the MCP client.
 *
 * Only meaningful in Tauri mode — the server returns 501 for take-screenshot
 * and never emits the event, so this is never called in web mode.
 */
export const notifyScreenshotCaptured = (
	request_id: string,
	info: ScreenshotInfo,
) => command<void>('notify_screenshot_captured', { request_id, info });

/** Acknowledges that the live UI received an MCP screenshot request. */
export const notifyScreenshotStarted = (request_id: string) =>
	command<void>('notify_screenshot_started', { request_id });

/**
 * Reports a screenshot capture failure back to the MCP bridge so it can
 * return an error to the MCP client immediately rather than waiting for the
 * 15-second timeout.
 *
 * Call this whenever `captureScreen()` or `saveScreenshot()` throws inside
 * the `onRequestScreenshot` handler.
 */
export const notifyScreenshotError = (request_id: string, error: string) =>
	command<void>('notify_screenshot_error', { request_id, error });

export interface RequestScreenshotPayload {
	request_id: string;
}

/** Registers a handler for agent-triggered screenshot requests. */
export const onRequestScreenshot = (
	cb: (payload: RequestScreenshotPayload) => void,
) => onEvent<RequestScreenshotPayload>('request-screenshot', cb);
