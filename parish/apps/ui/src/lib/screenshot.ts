/**
 * Screenshot helper — captures the current Svelte app shell as a PNG data URL.
 *
 * The illustrated game is already a complete Pixi canvas, so that surface is
 * captured directly. Other routes fall back to `html-to-image`, keeping the
 * helper portable across Tauri and web modes. The returned data URL is handed
 * to `saveScreenshot` in `ipc.ts`, which posts it to the backend so the PNG
 * lands under `<saves_dir>/screenshots/`.
 */

import { toPng } from 'html-to-image';

/**
 * Captures the visible app shell and returns a `data:image/png;base64,...` URL.
 *
 * Targets `.app-shell` first (the top-level wrapper in `+page.svelte`); falls
 * back to `document.body` if that selector is absent (e.g. the editor route).
 * Throws on rendering failure so the caller can surface it via the error log.
 */
export async function captureScreen(): Promise<string> {
	const illustratedCanvas = document.querySelector<HTMLCanvasElement>(
		'[data-testid="illustrated-notebook-pixi-host"] canvas',
	);
	if (
		illustratedCanvas &&
		!document.querySelector('[role="dialog"]') &&
		illustratedCanvas.width > 1 &&
		illustratedCanvas.height > 1
	) {
		try {
			const dataUrl = illustratedCanvas.toDataURL('image/png');
			if (dataUrl.startsWith('data:image/png')) return dataUrl;
		} catch {
			// A tainted/unavailable canvas still gets the portable DOM fallback.
		}
	}

	const target =
		(document.querySelector('.app-shell') as HTMLElement | null) ??
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
