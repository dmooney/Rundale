/**
 * Playwright fixtures that mock the Tauri IPC layer.
 *
 * Injects a fake `window.__TAURI_INTERNALS__` so `@tauri-apps/api` calls
 * resolve with test data. Also handles the event plugin protocol:
 * `plugin:event|listen` registers callbacks, and our `__TEST_EMIT_EVENT__`
 * helper dispatches to them.
 */

import { expect, test as base, type Page } from '@playwright/test';
import {
	SNAPSHOTS,
	PALETTES,
	MAP_DATA,
	NPCS,
	UI_CONFIG,
	DEBUG_SNAPSHOT,
	SAVE_FILES,
	SAVE_STATE,
	SETUP_SNAPSHOT,
	EDITOR_MODS,
	EDITOR_SNAPSHOT,
} from './mock-data';
import type {
	MapData,
	NpcInfo,
	ThemePalette,
	TextLogEntry,
	UiConfig,
	WorldSnapshot,
} from '../src/lib/types';

/** Minimal 1×1 transparent PNG — fulfills tile requests instantly. */
const BLANK_PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
	'base64',
);

/**
 * Intercept MapLibre tile requests so the tile proxy never contacts S3.
 * Must be called before `page.goto()`.
 */
export async function installTileRouteMock(page: Page): Promise<void> {
	await page.route('**/tiles/**', (route) =>
		route.fulfill({ status: 200, contentType: 'image/png', body: BLANK_PNG }),
	);
}

/**
 * Wait until Pixi has presented a texture-complete frame, not merely appended
 * its canvas and emitted accessibility hit targets. The renderer loads assets
 * asynchronously and the GPU upload/present can trail those DOM-ready signals;
 * a raw Playwright screenshot taken in that gap contains large black texture
 * rectangles. Sample the presented WebGL canvas from a requestAnimationFrame
 * callback and fail closed unless the authored scene is both predominantly
 * non-black and chromatically varied.
 */
export async function waitForTextureCompleteNotebookFrame(
	page: Page,
): Promise<void> {
	const canvas = page
		.getByTestId('illustrated-notebook-pixi-host')
		.locator('canvas');
	const preservesPresentedFrame = await canvas.evaluate((element) => {
		const source = element as HTMLCanvasElement;
		const context = source.getContext('webgl2') ?? source.getContext('webgl');
		return context?.getContextAttributes()?.preserveDrawingBuffer ?? null;
	});
	expect(
		preservesPresentedFrame,
		'Pixi WebGL must preserve its presented frame for product and proof captures',
	).toBe(true);

	await expect
		.poll(
			() =>
				canvas.evaluate(
					(element) =>
						new Promise<boolean>((resolve) => {
							requestAnimationFrame(() => {
								const source = element as HTMLCanvasElement;
								const sample = document.createElement('canvas');
								sample.width = 32;
								sample.height = 20;
								const context = sample.getContext('2d', {
									willReadFrequently: true,
								});
								if (!context) {
									resolve(false);
									return;
								}

								context.drawImage(source, 0, 0, sample.width, sample.height);
								const pixels = context.getImageData(
									0,
									0,
									sample.width,
									sample.height,
								).data;
								let nonBlack = 0;
								const colourBuckets = new Set<number>();
								for (let i = 0; i < pixels.length; i += 4) {
									const red = pixels[i];
									const green = pixels[i + 1];
									const blue = pixels[i + 2];
									const alpha = pixels[i + 3];
									if (alpha > 0 && red + green + blue > 60) nonBlack += 1;
									colourBuckets.add(
										(red >> 4) * 256 + (green >> 4) * 16 + (blue >> 4),
									);
								}

								const pixelCount = pixels.length / 4;
								resolve(
									nonBlack / pixelCount >= 0.8 && colourBuckets.size >= 20,
								);
							});
						}),
				),
			{
				message:
					'Pixi notebook must present a texture-complete, non-degenerate frame',
				timeout: 10_000,
			},
		)
		.toBe(true);
}

/**
 * Inject the Tauri IPC mock into a page before navigation.
 * Must be called before `page.goto()`.
 */
export async function installTauriMock(
	page: Page,
	timeOfDay: string = 'morning',
	options?: {
		debugSnapshot?: unknown;
		saveFiles?: unknown;
		saveState?: unknown;
		snapshot?: WorldSnapshot;
		mapData?: MapData;
		npcs?: NpcInfo[];
		uiConfig?: UiConfig;
	},
): Promise<void> {
	await installTileRouteMock(page);
	const snapshot = options?.snapshot ?? SNAPSHOTS[timeOfDay];
	const palette = PALETTES.default;
	const mapData = options?.mapData ?? MAP_DATA;
	const npcs = options?.npcs ?? NPCS;
	const uiConfig = options?.uiConfig ?? UI_CONFIG;
	const debugSnapshot = options?.debugSnapshot ?? DEBUG_SNAPSHOT;
	const saveFiles = options?.saveFiles ?? SAVE_FILES;
	const saveState = options?.saveState ?? SAVE_STATE;
	const setupSnapshot = SETUP_SNAPSHOT;
	const editorMods = EDITOR_MODS;
	const editorSnapshot = EDITOR_SNAPSHOT;

	await page.addInitScript(
		({
			snapshot,
			palette,
			mapData,
			npcs,
			uiConfig,
			debugSnapshot,
			saveFiles,
			saveState,
			setupSnapshot,
			editorMods,
			editorSnapshot,
		}) => {
			// ── Callback registry (mirrors Tauri's transformCallback) ────────
			const callbacks: Record<number, (data: unknown) => void> = {};
			let nextCallbackId = 1;

			// ── Event listener registry ─────────────────────────────────────
			// Maps event name → array of { id, callbackId }
			const eventListeners: Record<
				string,
				Array<{ id: number; callbackId: number }>
			> = {};
			let nextEventId = 1;

			// ── Mock invoke responses ───────────────────────────────────────
			const mockResponses: Record<string, unknown> = {
				get_world_snapshot: snapshot,
				get_map: mapData,
				get_npcs_here: npcs,
				get_theme: palette,
				get_ui_config: uiConfig,
				get_debug_snapshot: debugSnapshot,
				discover_save_files: saveFiles,
				get_save_state: saveState,
				get_setup_snapshot: setupSnapshot,
				editor_list_mods: editorMods,
				editor_open_mod: editorSnapshot,
			};

			// Expose for test helpers
			(window as unknown as Record<string, unknown>).__TEST_MOCK_RESPONSES__ =
				mockResponses;

			// ── Test event emitter ──────────────────────────────────────────
			(window as unknown as Record<string, unknown>).__TEST_EMIT_EVENT__ = (
				event: string,
				payload: unknown,
			) => {
				const listeners = eventListeners[event] || [];
				for (const listener of listeners) {
					const cb = callbacks[listener.callbackId];
					if (cb) {
						// Tauri event shape: { event, id, payload }
						cb({ event, id: listener.id, payload });
					}
				}
			};

			// ── __TAURI_EVENT_PLUGIN_INTERNALS__ ────────────────────────────
			(
				window as unknown as Record<string, unknown>
			).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
				unregisterListener: (event: string, eventId: number) => {
					if (eventListeners[event]) {
						eventListeners[event] = eventListeners[event].filter(
							(l: { id: number }) => l.id !== eventId,
						);
					}
				},
			};

			// ── __TAURI_INTERNALS__ ─────────────────────────────────────────
			(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
				transformCallback: (
					callback: (data: unknown) => void,
					_once?: boolean,
				) => {
					const id = nextCallbackId++;
					callbacks[id] = callback;
					return id;
				},

				unregisterCallback: (id: number) => {
					delete callbacks[id];
				},

				invoke: async (cmd: string, args?: Record<string, unknown>) => {
					// Handle event plugin commands
					if (cmd === 'plugin:event|listen') {
						const event = args?.event as string;
						const callbackId = args?.handler as number;
						const eventId = nextEventId++;
						if (!eventListeners[event]) {
							eventListeners[event] = [];
						}
						eventListeners[event].push({ id: eventId, callbackId });
						return eventId;
					}
					if (cmd === 'plugin:event|unlisten') {
						const event = args?.event as string;
						const eventId = args?.eventId as number;
						if (eventListeners[event]) {
							eventListeners[event] = eventListeners[event].filter(
								(l: { id: number }) => l.id !== eventId,
							);
						}
						return;
					}
					if (cmd === 'plugin:event|emit' || cmd === 'plugin:event|emit_to') {
						return;
					}

					// Handle app commands
					if (cmd in mockResponses) {
						return mockResponses[cmd];
					}

					// submit_input and other commands: no-op
					return null;
				},

				metadata: {
					currentWindow: { label: 'main' },
					currentWebview: { label: 'main' },
				},

				convertFileSrc: (path: string) => path,
			};
		},
		{
			snapshot,
			palette,
			mapData,
			npcs,
			uiConfig,
			debugSnapshot,
			saveFiles,
			saveState,
			setupSnapshot,
			editorMods,
			editorSnapshot,
		},
	);
}

/**
 * Emit a Tauri event into the page (triggers registered listeners).
 */
export async function emitEvent(
	page: Page,
	event: string,
	payload: unknown,
): Promise<void> {
	await page.evaluate(
		({ event, payload }) => {
			const emit = (
				window as unknown as Record<string, (e: string, p: unknown) => void>
			).__TEST_EMIT_EVENT__;
			if (emit) emit(event, payload);
		},
		{ event, payload },
	);
}

/**
 * Update a mock invoke response (does not trigger UI update — emit an event after).
 */
export async function updateMockResponse(
	page: Page,
	command: string,
	data: unknown,
): Promise<void> {
	await page.evaluate(
		({ command, data }) => {
			const responses = (
				window as unknown as Record<string, Record<string, unknown>>
			).__TEST_MOCK_RESPONSES__;
			if (responses) responses[command] = data;
		},
		{ command, data },
	);
}

/**
 * Apply a theme palette by emitting a theme-update event.
 */
export async function applyTheme(
	page: Page,
	palette: ThemePalette,
): Promise<void> {
	await emitEvent(page, 'theme-update', palette);
}

/**
 * Add a text log entry by emitting a text-log event.
 */
export async function addTextLog(
	page: Page,
	entry: TextLogEntry,
): Promise<void> {
	await emitEvent(page, 'text-log', {
		source: entry.source,
		content: entry.content,
	});
}

// ── Extended test fixture ───────────────────────────────────────────────────

export const test = base.extend<{
	parishPage: Page;
}>({
	parishPage: async ({ page }, use) => {
		await installTauriMock(page, 'morning');
		await page.goto('/');
		await page.waitForLoadState('networkidle');
		await use(page);
	},
});

export { expect };
