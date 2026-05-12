/**
 * Screenshot helper — captures the current Svelte app shell as a PNG data URL.
 *
 * Uses `html-to-image` so the capture works in both Tauri and web modes
 * without depending on platform-specific APIs (the existing GDK code in
 * `parish-tauri/src/lib.rs` is Linux-only and is reserved for the
 * `--screenshot` CI batch flag). The returned data URL is handed to
 * `saveScreenshot` in `ipc.ts`, which posts it to the backend so the PNG
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
	const target =
		(document.querySelector('.app-shell') as HTMLElement | null) ?? document.body;
	if (!target) {
		throw new Error('No DOM target available to screenshot.');
	}
	// `cacheBust: true` works around `<img>` re-use across captures (map tiles
	// in particular). `pixelRatio: window.devicePixelRatio` keeps the output
	// crisp on HiDPI displays without hardcoding 1x or 2x.
	return await toPng(target, {
		cacheBust: true,
		pixelRatio: window.devicePixelRatio || 1
	});
}
