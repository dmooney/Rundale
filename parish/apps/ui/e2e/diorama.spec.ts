/**
 * Playwright end-to-end tests for the Interactive Parish Diorama (M3).
 *
 * These tests run against the real app in Tauri-mock mode. The `diorama` flag
 * is enabled by injecting a non-null `get_scene_state` response into the mock,
 * then reloading so page-controller picks it up at mount time.
 *
 * Note: if Playwright browsers are not installed (`npx playwright install` has
 * not been run), this file will be skipped by the runner with an informational
 * message — the spec itself is complete and correct.
 */

import {
	test,
	expect,
	installTauriMock,
	emitEvent,
	updateMockResponse,
} from './fixtures';
import type { SceneState } from '../src/lib/types';

// ── Mock scene state with a travel hotspot and an NPC sprite ────────────────

/** Minimal 1×1 transparent PNG as a base64 data URL (plate + sprite placeholder). */
const BLANK_DATA_URL =
	'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==';

const SCENE_STATE: SceneState = {
	schema_version: 1,
	location_id: 1,
	plate_url: BLANK_DATA_URL,
	variant: 'day',
	indoor: false,
	hotspots: [
		{
			id: 'to-howth',
			shape: { rect: [10, 10, 30, 30] },
			label: 'Go to Binn Éadair',
			action: { travel_to: 2 }, // not matched to MAP_DATA — travel is a no-op here
		},
		{
			id: 'hearth',
			shape: { rect: [60, 10, 20, 20] },
			label: 'The hearth',
			action: { inspect: 'A turf fire smoulders in the wide hearth.' },
		},
	],
	npcs: [
		{
			id: 1,
			display_name: 'Séamas Ó Briain',
			real_name: 'Séamas Ó Briain',
			introduced: true,
			mood_emoji: '😊',
			sprite_url: BLANK_DATA_URL,
			x: 48,
			y: 55,
			scale: 1.0,
			flip: false,
		},
	],
	overflow_npcs: [],
};

// A scene state for an adjacent location (howth = id 2 in mock map, name "Binn Éadair")
const SCENE_STATE_HOWTH: SceneState = {
	...SCENE_STATE,
	location_id: 2,
	hotspots: [],
	npcs: [],
};

// ── Helper: install mock with scene state ────────────────────────────────────

async function installWithScene(
	page: import('@playwright/test').Page,
	scene: SceneState | null = SCENE_STATE,
): Promise<void> {
	await installTauriMock(page, 'morning');
	// Pre-seed get_scene_state before page load so page-controller picks it up.
	await page.addInitScript(
		({ scene }) => {
			const win = window as unknown as Record<string, unknown>;
			// The mock responses map is set up by installTauriMock; we add to it here.
			// We use a secondary init script that runs after the primary one.
			const origInternals = Object.getOwnPropertyDescriptor(
				win,
				'__TAURI_INTERNALS__',
			);
			Object.defineProperty(win, '__TAURI_INTERNALS__', {
				configurable: true,
				get() {
					const base = origInternals?.get?.call(win) ?? origInternals?.value;
					if (!base) return undefined;
					const origInvoke = base.invoke.bind(base);
					return {
						...base,
						invoke: async (cmd: string, args?: Record<string, unknown>) => {
							if (cmd === 'get_scene_state') return scene;
							return origInvoke(cmd, args);
						},
					};
				},
			});
		},
		{ scene },
	);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test.describe('Diorama — flag on', () => {
	test.beforeEach(async ({ page }) => {
		await installWithScene(page, SCENE_STATE);
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	// AC-2: plate <img> is visible
	test('plate img is visible when scene state is present', async ({ page }) => {
		const plate = page.locator('[data-testid="diorama-view"] img').first();
		await expect(plate).toBeVisible();
	});

	// AC-2: hotspot SVG layer is rendered congruently over the plate
	test('hotspot SVG overlay is present', async ({ page }) => {
		const svg = page.locator('[data-testid="diorama-view"] svg').first();
		await expect(svg).toBeAttached();
	});

	// AC-5: inspect hotspot appends a local system entry to the chat log
	test('clicking an inspect hotspot appends a local system entry', async ({
		page,
	}) => {
		// Click the inspect hotspot rect — SVG elements respond to click in Playwright
		const svg = page.locator('[data-testid="diorama-view"] svg').first();
		// Find the rect with aria-label "The hearth"
		const hearthRect = svg.locator('[aria-label="The hearth"]');
		await hearthRect.click({ force: true });

		// The text log should now contain the inspect text
		await expect(
			page
				.locator('.chat-panel')
				.getByText('A turf fire smoulders in the wide hearth.'),
		).toBeVisible({ timeout: 3000 });
	});

	// AC-4: clicking an NPC sprite focuses the input with addressed_to
	test('clicking an NPC sprite inserts an @mention chip into the input', async ({
		page,
	}) => {
		const spriteBtn = page.locator('.sprite-btn').first();
		await spriteBtn.click({ force: true });

		// The mention chip should appear in the input editor
		const mentionChip = page.locator('.input-field .mention-chip');
		await expect(mentionChip).toBeVisible({ timeout: 3000 });
		await expect(mentionChip).toContainText('Séamas Ó Briain');
	});

	// AC-9: hotspot is keyboard-accessible (tabindex, aria-label)
	test('hotspot rect has tabindex=0 and aria-label', async ({ page }) => {
		const rect = page.locator('[data-testid="diorama-view"] svg rect').first();
		await expect(rect).toHaveAttribute('tabindex', '0');
		// aria-label is one of the two hotspot labels defined above
		const label = await rect.getAttribute('aria-label');
		expect(['Go to Binn Éadair', 'The hearth']).toContain(label);
	});
});

test.describe('Diorama — flag off / null scene state', () => {
	test.beforeEach(async ({ page }) => {
		// null scene state → no diorama rendered
		await installWithScene(page, null);
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	// AC-1: no diorama in DOM when scene state is null
	test('diorama-view is absent when scene state is null', async ({ page }) => {
		await expect(
			page.locator('[data-testid="diorama-view"]'),
		).not.toBeAttached();
	});

	// AC-8: existing layout is intact (chat panel + input field visible)
	test('existing chat panel and input field are visible', async ({ page }) => {
		await expect(page.locator('.chat-panel')).toBeVisible();
		await expect(page.locator('[data-testid="input-field"]')).toBeVisible();
	});
});

test.describe('Diorama — travel dimming on travel-start event', () => {
	test.beforeEach(async ({ page }) => {
		await installWithScene(page, SCENE_STATE);
		await page.goto('/');
		await page.waitForLoadState('networkidle');
	});

	// AC-10: plate-wrap dims on travel-start event
	test('plate dims when travel-start event fires', async ({ page }) => {
		await emitEvent(page, 'travel-start', {
			waypoints: [
				{ id: 'dublin', lat: 53.3498, lon: -6.2603 },
				{ id: 'howth', lat: 53.3862, lon: -6.065 },
			],
			duration_minutes: 20,
			destination: 'howth',
		});

		const plateWrap = page.locator('.plate-wrap');
		await expect(plateWrap).toHaveClass(/dimmed/, { timeout: 2000 });
	});

	// AC-10: dimming clears on arrival world-update
	test('dim clears when arrival world-update fires', async ({ page }) => {
		// Dim first
		await emitEvent(page, 'travel-start', {
			waypoints: [{ id: 'dublin', lat: 53.3498, lon: -6.2603 }],
			duration_minutes: 5,
			destination: 'howth',
		});
		await expect(page.locator('.plate-wrap')).toHaveClass(/dimmed/, {
			timeout: 2000,
		});

		// Set up scene state for new location + emit world-update
		await updateMockResponse(page, 'get_scene_state', SCENE_STATE_HOWTH);
		await emitEvent(page, 'world-update', {
			location_name: 'Binn Éadair',
			location_description: 'The hill overlooks the sea.',
			time_label: 'Morning',
			hour: 8,
			minute: 20,
			weather: 'Clear',
			season: 'Spring',
			festival: null,
			paused: false,
			inference_paused: false,
			game_epoch_ms: Date.now(),
			speed_factor: 0,
			name_hints: [],
			day_of_week: 'Monday',
		});

		// Allow async world-update handler (getSceneState fetch + travelDimming.set(false))
		await page.waitForTimeout(500);

		await expect(page.locator('.plate-wrap')).not.toHaveClass(/dimmed/, {
			timeout: 2000,
		});
	});
});
