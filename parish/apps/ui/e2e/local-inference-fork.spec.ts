/**
 * SetupOverlay → LocalInferenceFork rendering.
 *
 * Mocks the Tauri side so:
 *   - `get_setup_snapshot` reports `needs_onboarding: true` (gate fired).
 *   - `get_onboarding_options` reports `local-recommended` on a 48 GB host.
 *
 * Then screenshots the rendered fork to `docs/screenshots/onboarding-local-inference.png`
 * so the proof bundle has the visible UI element that drives the live
 * download / vllm-mlx-serve flow proven end-to-end via the MCP bridge.
 *
 * Run: npx playwright test e2e/local-inference-fork.spec.ts
 */

import { test, installTauriMock, applyTheme, updateMockResponse, emitEvent } from './fixtures';
import { PALETTES } from './mock-data';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SCREENSHOT_DIR = path.resolve(__dirname, '../../../../docs/screenshots');

test('LocalInferenceFork renders on a Mac with sufficient memory', async ({ page }) => {
	await installTauriMock(page, 'morning');

	// Before goto: pre-set the mock responses the SetupOverlay reads on mount.
	await page.addInitScript(() => {
		const responses = (window as unknown as Record<string, Record<string, unknown>>)
			.__TEST_MOCK_RESPONSES__;
		if (responses) {
			responses['get_setup_snapshot'] = {
				current_message: 'Preparing the storyteller...',
				messages: ['Preparing the storyteller...'],
				completed: 0,
				total: 0,
				done: false,
				success: null,
				error: '',
				needs_onboarding: true,
				onboarding_choice: 'local-recommended'
			};
			responses['get_onboarding_options'] = {
				choice: 'local-recommended',
				ram_gb: 48
			};
		}
	});

	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await applyTheme(page, PALETTES.morning);

	// Some mock responses are set after the bundle loads; reset them and
	// emit the gate event so the SetupOverlay actually mounts the fork.
	await updateMockResponse(page, 'get_setup_snapshot', {
		current_message: 'Preparing the storyteller...',
		messages: ['Preparing the storyteller...'],
		completed: 0,
		total: 0,
		done: false,
		success: null,
		error: '',
		needs_onboarding: true,
		onboarding_choice: 'local-recommended'
	});
	await updateMockResponse(page, 'get_onboarding_options', {
		choice: 'local-recommended',
		ram_gb: 48
	});
	await emitEvent(page, 'setup-needs-onboarding', {
		message: 'Awaiting provider choice'
	});

	// Wait for the fork copy to render (any string unique to LocalInferenceFork).
	await page.waitForSelector('text=Run locally', { timeout: 5000 });

	await page.screenshot({
		path: path.join(SCREENSHOT_DIR, 'onboarding-local-inference.png'),
		fullPage: false
	});
});
