/**
 * Theme-variant screenshots for PR #1064 (zork + solarized asset mods).
 * Outputs to docs/screenshots/theme-<name>-<mode>.png.
 *
 * Run: npx playwright test e2e/screenshots-themes.spec.ts
 */

import { test, expect, addTextLog } from './fixtures';
import {
	TEXT_LOG,
	UI_CONFIG,
	SNAPSHOTS,
	MAP_DATA,
	NPCS,
	PALETTES,
	DEBUG_SNAPSHOT,
	SAVE_FILES,
	SAVE_STATE,
	SETUP_SNAPSHOT,
	EDITOR_MODS,
	EDITOR_SNAPSHOT
} from './mock-data';
import type { Page } from '@playwright/test';
import type { ThemePalette } from '../src/lib/types';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// `e2e/` → `apps/ui/` → `apps/` → `parish/` → repo root → `docs/screenshots/`.
const SCREENSHOT_DIR = path.resolve(__dirname, '../../../../docs/screenshots');

const MONO_FONT = "'Courier New', Consolas, ui-monospace, monospace";

const THEMES: Record<string, ThemePalette> = {
	'zork-c64': {
		bg: '#1f1b96',
		fg: '#a8a8ff',
		accent: '#ffffff',
		panel_bg: '#1f1b96',
		input_bg: '#15116b',
		border: '#7878ff',
		muted: '#6c5eb5',
		font_body: MONO_FONT,
		font_display: MONO_FONT,
		chat_align: 'left',
		bubble_style: 'flat',
		status_invert: true,
		decoration: 'none'
	},
	'zork-dos': {
		bg: '#000000',
		fg: '#c0c0c0',
		accent: '#ffff55',
		panel_bg: '#000000',
		input_bg: '#0a0a0a',
		border: '#808080',
		muted: '#808080',
		font_body: MONO_FONT,
		font_display: MONO_FONT,
		chat_align: 'left',
		bubble_style: 'flat',
		status_invert: true,
		decoration: 'none'
	},
	'solarized-light': {
		bg: '#fdf6e3',
		fg: '#586e75',
		accent: '#268bd2',
		panel_bg: '#eee8d5',
		input_bg: '#e6dfc5',
		border: '#93a1a1',
		muted: '#93a1a1',
		decoration: 'none'
	},
	'solarized-dark': {
		bg: '#002b36',
		fg: '#839496',
		accent: '#268bd2',
		panel_bg: '#073642',
		input_bg: '#0d3f4f',
		border: '#586e75',
		muted: '#586e75',
		decoration: 'none'
	}
};

async function applyPaletteDirect(page: Page, palette: ThemePalette): Promise<void> {
	await page.evaluate((p) => {
		const root = document.documentElement;
		root.style.setProperty('--color-bg', p.bg);
		root.style.setProperty('--color-fg', p.fg);
		root.style.setProperty('--color-accent', p.accent);
		root.style.setProperty('--color-panel-bg', p.panel_bg);
		root.style.setProperty('--color-input-bg', p.input_bg);
		root.style.setProperty('--color-border', p.border);
		root.style.setProperty('--color-muted', p.muted);
		if (p.font_body) root.style.setProperty('--font-body', p.font_body);
		if (p.font_display) root.style.setProperty('--font-display', p.font_display);
		if (p.chat_align === 'left') {
			root.style.setProperty('--bubble-player-justify', 'flex-start');
			root.style.setProperty('--label-player-text-align', 'left');
			root.style.setProperty('--label-player-padding-right', '0');
			root.style.setProperty('--label-player-padding-left', '0.75rem');
		}
		if (p.bubble_style === 'flat') {
			root.style.setProperty('--bubble-npc-bg', 'transparent');
			root.style.setProperty('--bubble-npc-fg', p.fg);
			root.style.setProperty('--bubble-npc-border-left', 'none');
			root.style.setProperty('--bubble-npc-radius', '0');
			root.style.setProperty('--bubble-player-bg', 'transparent');
			root.style.setProperty('--bubble-player-fg', p.fg);
			root.style.setProperty('--bubble-player-radius', '0');
			root.style.setProperty('--bubble-font-style', 'normal');
			root.style.setProperty('--color-irish', p.fg);
			root.style.setProperty('--color-name', p.accent);
			root.style.setProperty('--color-location', p.accent);
			root.style.setProperty('--npc-chips-bg', p.panel_bg);
		}
		if (p.decoration === 'none') {
			root.style.setProperty('--decoration-display', 'none');
		}
		if (p.status_invert) {
			root.style.setProperty('--status-bg', p.fg);
			root.style.setProperty('--status-fg', p.bg);
			root.style.setProperty('--status-accent-fg', p.bg);
			root.style.setProperty('--status-muted-fg', p.bg);
			root.style.setProperty('--status-border', p.bg);
			root.style.setProperty('--status-sep-fg', p.bg);
			root.style.setProperty('--status-clock-bg', p.fg);
			root.style.setProperty('--status-clock-fg', p.bg);
			root.style.setProperty('--status-border-bottom', `2px solid ${p.fg}`);
		}
	}, palette);
}

async function installThemeMock(page: Page): Promise<void> {
	const snapshot = SNAPSHOTS['midday'];
	const palette = PALETTES.default;
	const uiConfig = {
		...UI_CONFIG,
		active_tile_source: 'historic',
		tile_sources: [
			{
				id: 'historic',
				label: 'Historic 6" OS Ireland (1st ed., via NLS)',
				url: '/tiles/historic/{z}/{x}/{y}.png',
				tile_size: 256,
				minzoom: 0,
				maxzoom: 19,
				attribution: '© National Library of Scotland (CC-BY)',
				raster_saturation: 0,
				raster_opacity: 1,
				tms: false
			}
		]
	};

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
			editorSnapshot
		}) => {
			const callbacks: Record<number, (data: unknown) => void> = {};
			let nextCallbackId = 1;
			const eventListeners: Record<string, Array<{ id: number; callbackId: number }>> = {};
			let nextEventId = 1;
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
				editor_open_mod: editorSnapshot
			};
			(window as unknown as Record<string, unknown>).__TEST_MOCK_RESPONSES__ = mockResponses;
			(window as unknown as Record<string, unknown>).__TEST_EMIT_EVENT__ = (
				event: string,
				payload: unknown
			) => {
				const listeners = eventListeners[event] || [];
				for (const listener of listeners) {
					const cb = callbacks[listener.callbackId];
					if (cb) cb({ event, id: listener.id, payload });
				}
			};
			(window as unknown as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
				unregisterListener: (event: string, eventId: number) => {
					if (eventListeners[event]) {
						eventListeners[event] = eventListeners[event].filter((l) => l.id !== eventId);
					}
				}
			};
			(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {
				transformCallback: (callback: (data: unknown) => void) => {
					const id = nextCallbackId++;
					callbacks[id] = callback;
					return id;
				},
				unregisterCallback: (id: number) => {
					delete callbacks[id];
				},
				invoke: async (cmd: string, args?: Record<string, unknown>) => {
					if (cmd === 'plugin:event|listen') {
						const event = args?.event as string;
						const callbackId = args?.handler as number;
						const eventId = nextEventId++;
						if (!eventListeners[event]) eventListeners[event] = [];
						eventListeners[event].push({ id: eventId, callbackId });
						return eventId;
					}
					if (cmd === 'plugin:event|unlisten') {
						const event = args?.event as string;
						const eventId = args?.eventId as number;
						if (eventListeners[event]) {
							eventListeners[event] = eventListeners[event].filter((l) => l.id !== eventId);
						}
						return;
					}
					if (cmd === 'plugin:event|emit' || cmd === 'plugin:event|emit_to') return;
					if (cmd in mockResponses) return mockResponses[cmd];
					return null;
				},
				metadata: {
					currentWindow: { label: 'main' },
					currentWebview: { label: 'main' }
				},
				convertFileSrc: (path: string) => path
			};
		},
		{
			snapshot,
			palette,
			mapData: MAP_DATA,
			npcs: NPCS,
			uiConfig,
			debugSnapshot: DEBUG_SNAPSHOT,
			saveFiles: SAVE_FILES,
			saveState: SAVE_STATE,
			setupSnapshot: SETUP_SNAPSHOT,
			editorMods: EDITOR_MODS,
			editorSnapshot: EDITOR_SNAPSHOT
		}
	);
}

test.describe('Theme screenshots', () => {
	for (const [name, palette] of Object.entries(THEMES)) {
		test(`capture theme-${name}`, async ({ page }) => {
			await installThemeMock(page);

			await page.goto('/');
			await page.waitForLoadState('networkidle');

			for (const entry of TEXT_LOG) {
				await addTextLog(page, entry);
			}

			await expect(page.locator('[data-testid="chat-panel"]')).toContainText(
				TEXT_LOG[TEXT_LOG.length - 1].content
			);

			// Apply palette AFTER onMount completes (text-log presence implies
			// onMount has run past the get_theme/hydrate path that would otherwise
			// overwrite our CSS variables).
			// Disable the 1.5s color transitions so the screenshot reflects the
			// final palette immediately (default app.css transitions `background`,
			// `color`, and `border-color` on every element).
			await page.addStyleTag({
				content: '*, *::before, *::after { transition: none !important; }'
			});
			await applyPaletteDirect(page, palette);
			// Give MapLibre time to fetch + paint historic tiles.
			await page.waitForTimeout(2500);

			await page.screenshot({
				path: path.join(SCREENSHOT_DIR, `theme-${name}.png`),
				fullPage: false
			});
		});
	}
});
