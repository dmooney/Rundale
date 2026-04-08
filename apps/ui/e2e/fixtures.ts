/**
 * Playwright fixtures that mock the Tauri IPC layer.
 *
 * Injects a fake `window.__TAURI_INTERNALS__` so `@tauri-apps/api` calls
 * resolve with test data. Also handles the event plugin protocol:
 * `plugin:event|listen` registers callbacks, and our `__TEST_EMIT_EVENT__`
 * helper dispatches to them.
 */

import { test as base, type Page } from '@playwright/test';
import {
	SNAPSHOTS,
	PALETTES,
	MAP_DATA,
	NPCS,
	IRISH_HINTS,
	TEXT_LOG
} from './mock-data';
import type { ThemePalette, WorldSnapshot, TextLogEntry } from '../src/lib/types';

/**
 * Inject the Tauri IPC mock into a page before navigation.
 * Must be called before `page.goto()`.
 */
export async function installTauriMock(
	page: Page,
	timeOfDay: string = 'morning'
): Promise<void> {
	const snapshot = SNAPSHOTS[timeOfDay];
	const palette = PALETTES[timeOfDay];
	const mapData = MAP_DATA;
	const npcs = NPCS;

	await page.addInitScript(
		({ snapshot, palette, mapData, npcs }) => {
			// ── Callback registry (mirrors Tauri's transformCallback) ────────
			const callbacks: Record<number, (data: unknown) => void> = {};
			let nextCallbackId = 1;

			// ── Event listener registry ─────────────────────────────────────
			// Maps event name → array of { id, callbackId }
			const eventListeners: Record<string, Array<{ id: number; callbackId: number }>> = {};
			let nextEventId = 1;

			// ── Mock invoke responses ───────────────────────────────────────
			const mockResponses: Record<string, unknown> = {
				get_world_snapshot: snapshot,
				get_map: mapData,
				get_npcs_here: npcs,
				get_theme: palette,
				get_ui_config: {
					hints_label: 'Focail',
					default_accent: '#c4a35a',
					splash_text: 'Parish: Kilteevan 1820\nCopyright \u00A9 2026 David Mooney. All rights reserved.\ntest-branch - 2026-03-29 00:00'
				}
			};

			// Expose for test helpers
			(window as unknown as Record<string, unknown>).__TEST_MOCK_RESPONSES__ = mockResponses;

			// ── Test event emitter ──────────────────────────────────────────
			(window as unknown as Record<string, unknown>).__TEST_EMIT_EVENT__ = (
				event: string,
				payload: unknown
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
			(window as unknown as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
				unregisterListener: (event: string, eventId: number) => {
					if (eventListeners[event]) {
						eventListeners[event] = eventListeners[event].filter(
							(l: { id: number }) => l.id !== eventId
						);
					}
				}
			};

			// ── __TAURI_INTERNALS__ ─────────────────────────────────────────
			(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
				transformCallback: (callback: (data: unknown) => void, _once?: boolean) => {
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
								(l: { id: number }) => l.id !== eventId
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
					currentWebview: { label: 'main' }
				},

				convertFileSrc: (path: string) => path
			};
		},
		{ snapshot, palette, mapData, npcs }
	);
}

/**
 * Emit a Tauri event into the page (triggers registered listeners).
 */
export async function emitEvent(page: Page, event: string, payload: unknown): Promise<void> {
	await page.evaluate(
		({ event, payload }) => {
			const emit = (window as unknown as Record<string, (e: string, p: unknown) => void>)
				.__TEST_EMIT_EVENT__;
			if (emit) emit(event, payload);
		},
		{ event, payload }
	);
}

/**
 * Update a mock invoke response (does not trigger UI update — emit an event after).
 */
export async function updateMockResponse(
	page: Page,
	command: string,
	data: unknown
): Promise<void> {
	await page.evaluate(
		({ command, data }) => {
			const responses = (window as unknown as Record<string, Record<string, unknown>>)
				.__TEST_MOCK_RESPONSES__;
			if (responses) responses[command] = data;
		},
		{ command, data }
	);
}

/**
 * Apply a theme palette by emitting a theme-update event.
 */
export async function applyTheme(page: Page, palette: ThemePalette): Promise<void> {
	await emitEvent(page, 'theme-update', palette);
}

/**
 * Add a text log entry by emitting a text-log event.
 */
export async function addTextLog(page: Page, entry: TextLogEntry): Promise<void> {
	await emitEvent(page, 'text-log', { source: entry.source, content: entry.content });
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
	}
});

export { expect } from '@playwright/test';
