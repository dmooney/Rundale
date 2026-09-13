/**
 * Screenshot helper — captures the current Svelte app shell as a PNG data URL.
 *
 * The chat shell and coordinated surfaces are ordinary DOM, so the capture
 * includes the entire app root. The returned data URL is handed to
 * `saveScreenshot` in `ipc.ts`, which posts it to the backend so the PNG lands
 * under `<saves_dir>/screenshots/`.
 */

import { toPng } from 'html-to-image';

/**
 * Captures the visible app shell and returns a `data:image/png;base64,...` URL.
 *
 * Targets `[data-testid="app-root"]` first so active dialog/sheet state and the
 * shell beneath it are captured together; falls back to `.chat-game-shell`,
 * then `document.body` for other routes.
 * Throws on rendering failure so the caller can surface it via the error log.
 */
export async function captureScreen(): Promise<string> {
	const target =
		(document.querySelector(
			'[data-testid="app-root"]',
		) as HTMLElement | null) ??
		(document.querySelector('.chat-game-shell') as HTMLElement | null) ??
		document.body;
	if (!target) {
		throw new Error('No DOM target available to screenshot.');
	}
	// Keep the capture cheap so it completes within the backend deadline even
	// under live local-inference load (#1160). `cacheBust` is deliberately NOT
	// set: it appends a unique query to every `<img>`, forcing html-to-image to
	// re-fetch and inline each cross-origin map tile, which is the dominant cost
	// and pushed captures past the timeout. `pixelRatio` is capped at 2 so HiDPI
	// displays (devicePixelRatio 3+) don't multiply the pixels to encode.
	return await toPng(target, {
		pixelRatio: Math.min(window.devicePixelRatio || 1, 2),
	});
}
