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

import {
	test,
	expect,
	installTauriMock,
	applyTheme,
	updateMockResponse,
	emitEvent,
} from './fixtures';
import { PALETTES } from './mock-data';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SCREENSHOT_DIR = path.resolve(__dirname, '../../../../docs/screenshots');

test('LocalInferenceFork renders on a Mac with sufficient memory', async ({
	page,
}) => {
	await installTauriMock(page, 'morning');

	// Before goto: pre-set the mock responses the SetupOverlay reads on mount.
	await page.addInitScript(() => {
		const responses = (
			window as unknown as Record<string, Record<string, unknown>>
		).__TEST_MOCK_RESPONSES__;
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
				onboarding_choice: 'local-recommended',
			};
			responses['get_onboarding_options'] = {
				choice: 'local-recommended',
				ram_gb: 48,
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
		onboarding_choice: 'local-recommended',
	});
	await updateMockResponse(page, 'get_onboarding_options', {
		choice: 'local-recommended',
		ram_gb: 48,
	});
	await emitEvent(page, 'setup-needs-onboarding', {
		message: 'Awaiting provider choice',
	});

	// Wait for the fork copy to render (any string unique to LocalInferenceFork).
	await page.waitForSelector('text=Run locally', { timeout: 5000 });

	await page.screenshot({
		path: path.join(SCREENSHOT_DIR, 'onboarding-local-inference.png'),
		fullPage: false,
	});
});

test('Picking local inference flips overlay to progress UI and renders live progress', async ({
	page,
}) => {
	// Same setup as the screenshot test: gate fired, fork rendered.
	await installTauriMock(page, 'morning');

	await page.addInitScript(() => {
		const responses = (
			window as unknown as Record<string, Record<string, unknown>>
		).__TEST_MOCK_RESPONSES__;
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
				onboarding_choice: 'local-recommended',
			};
			responses['get_onboarding_options'] = {
				choice: 'local-recommended',
				ram_gb: 48,
			};
			// start_local_inference_setup: keep pending forever so the test
			// can verify the overlay flips to progress UI *before* the IPC
			// resolves. Without this the mock's default `return null`
			// resolves synchronously and we couldn't tell whether the UI
			// flip happened because of the fix (onComplete pre-await) or
			// because of the natural resolution path.
			responses['start_local_inference_setup'] = new Promise(() => {});
		}
	});

	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await applyTheme(page, PALETTES.morning);

	await updateMockResponse(page, 'get_setup_snapshot', {
		current_message: 'Preparing the storyteller...',
		messages: ['Preparing the storyteller...'],
		completed: 0,
		total: 0,
		done: false,
		success: null,
		error: '',
		needs_onboarding: true,
		onboarding_choice: 'local-recommended',
	});
	await updateMockResponse(page, 'get_onboarding_options', {
		choice: 'local-recommended',
		ram_gb: 48,
	});
	await emitEvent(page, 'setup-needs-onboarding', {
		message: 'Awaiting provider choice',
	});

	// Fork is visible.
	await page.waitForSelector('text=Run locally', { timeout: 5000 });

	// Click the local-inference card — the inner <button>'s click handler
	// is pickLocal; it should call onComplete() (flipping needsOnboarding
	// to false) before awaiting the IPC.
	await page.getByRole('button', { name: /Set up local inference/i }).click();

	// After click: fork should be gone, progress UI from SetupOverlay
	// should be visible. Before the fix, this assertion would fail
	// because the fork stayed mounted on the "Starting local inference
	// setup…" placeholder until the IPC resolved.
	await page.waitForSelector('text=Run locally', {
		state: 'detached',
		timeout: 2000,
	});
	const progressTrack = page.locator(
		'[role="progressbar"][aria-label="Setup progress"]',
	);
	await progressTrack.waitFor({ state: 'visible', timeout: 2000 });

	// Drive a status message + progress events; verify they land on the
	// overlay. This is the path that was silent before the fix.
	await emitEvent(page, 'setup-status', {
		message: 'Downloading mlx-community/Qwen2.5-14B-Instruct-4bit',
	});
	await emitEvent(page, 'setup-progress', {
		completed: 1_000_000,
		total: 2_000_000,
	});

	// Activity panel should show the status message.
	await page.waitForSelector(
		'text=Downloading mlx-community/Qwen2.5-14B-Instruct-4bit',
		{ timeout: 2000 },
	);

	// Progress bar fills to 50%.
	await page.waitForFunction(
		() => {
			const el = document.querySelector(
				'[role="progressbar"][aria-label="Setup progress"]',
			);
			if (!el) return false;
			const v = el.getAttribute('aria-valuenow');
			return v !== null && Number(v) >= 49 && Number(v) <= 51;
		},
		undefined,
		{ timeout: 2000 },
	);
});

test('MCP-driven setup auto-dismisses the fork on first setup-status event', async ({
	page,
}) => {
	// Same gated-onboarding setup as the screenshot test — fork is up.
	await installTauriMock(page, 'morning');

	await page.addInitScript(() => {
		const responses = (
			window as unknown as Record<string, Record<string, unknown>>
		).__TEST_MOCK_RESPONSES__;
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
				onboarding_choice: 'local-recommended',
			};
			responses['get_onboarding_options'] = {
				choice: 'local-recommended',
				ram_gb: 48,
			};
		}
	});

	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await applyTheme(page, PALETTES.morning);

	await updateMockResponse(page, 'get_setup_snapshot', {
		current_message: 'Preparing the storyteller...',
		messages: ['Preparing the storyteller...'],
		completed: 0,
		total: 0,
		done: false,
		success: null,
		error: '',
		needs_onboarding: true,
		onboarding_choice: 'local-recommended',
	});
	await updateMockResponse(page, 'get_onboarding_options', {
		choice: 'local-recommended',
		ram_gb: 48,
	});
	await emitEvent(page, 'setup-needs-onboarding', {
		message: 'Awaiting provider choice',
	});

	// Fork is visible before any setup activity.
	await page.waitForSelector('text=Run locally', { timeout: 5000 });

	// Simulate the MCP-driven path: a remote client posted
	// /api/start-local-inference, which makes the Rust SetupProgress
	// implementation emit setup-status / setup-progress without any UI
	// button click. The fork should auto-dismiss so the user can watch
	// the download progress.
	await emitEvent(page, 'setup-status', {
		message: 'Preparing model download…',
	});

	await page.waitForSelector('text=Run locally', {
		state: 'detached',
		timeout: 2000,
	});
	const progressTrack = page.locator(
		'[role="progressbar"][aria-label="Setup progress"]',
	);
	await progressTrack.waitFor({ state: 'visible', timeout: 2000 });

	await emitEvent(page, 'setup-progress', {
		completed: 4_500_000_000,
		total: 9_000_000_000,
	});
	await page.waitForFunction(
		() => {
			const el = document.querySelector(
				'[role="progressbar"][aria-label="Setup progress"]',
			);
			if (!el) return false;
			const v = el.getAttribute('aria-valuenow');
			return v !== null && Number(v) >= 49 && Number(v) <= 51;
		},
		undefined,
		{ timeout: 2000 },
	);
});

test('Mid-flight snapshot on UI reload skips the fork and resumes progress UI', async ({
	page,
}) => {
	// MCP-driven path, second variant: the UI mounted AFTER setup was
	// already underway (e.g. the desktop window reloaded mid-download).
	// The snapshot still says needs_onboarding=true (Rust only clears
	// the flag at the end of bootstrap), but completed > 0 / messages
	// reveal that setup is already running. The fork should not render.
	await installTauriMock(page, 'morning');

	await page.addInitScript(() => {
		const responses = (
			window as unknown as Record<string, Record<string, unknown>>
		).__TEST_MOCK_RESPONSES__;
		if (responses) {
			responses['get_setup_snapshot'] = {
				current_message: 'Downloading model-00001-of-00002.safetensors',
				messages: [
					'Preparing the storyteller...',
					'Preparing model download…',
					'Downloading model-00001-of-00002.safetensors',
				],
				completed: 2_500_000_000,
				total: 9_200_000_000,
				done: false,
				success: null,
				error: '',
				needs_onboarding: true,
				onboarding_choice: 'local-recommended',
			};
			responses['get_onboarding_options'] = {
				choice: 'local-recommended',
				ram_gb: 48,
			};
		}
	});

	await page.goto('/');
	await page.waitForLoadState('networkidle');
	await applyTheme(page, PALETTES.morning);

	// Re-publish the in-flight snapshot after the bundle's own mocks
	// installed; do NOT emit setup-needs-onboarding because we want the
	// initial getSetupSnapshot() call on mount to be the trigger.
	await updateMockResponse(page, 'get_setup_snapshot', {
		current_message: 'Downloading model-00001-of-00002.safetensors',
		messages: [
			'Preparing the storyteller...',
			'Preparing model download…',
			'Downloading model-00001-of-00002.safetensors',
		],
		completed: 2_500_000_000,
		total: 9_200_000_000,
		done: false,
		success: null,
		error: '',
		needs_onboarding: true,
		onboarding_choice: 'local-recommended',
	});

	// Progress UI should be visible; fork should not appear.
	const progressTrack = page.locator(
		'[role="progressbar"][aria-label="Setup progress"]',
	);
	await progressTrack.waitFor({ state: 'visible', timeout: 5000 });
	await expect(page.locator('text=Run locally')).toHaveCount(0);
});
