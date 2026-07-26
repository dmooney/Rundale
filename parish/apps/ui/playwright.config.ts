import { defineConfig, devices } from '@playwright/test';

import {
	playwrightWebServerConfig,
	resolvePlaywrightPort,
} from './scripts/playwright-worktree-server.js';

// Playwright evaluates the config again in worker processes; the resolver
// publishes the allocated value so every evaluation uses the same port.
const testPort = await resolvePlaywrightPort();

/**
 * Playwright configuration for Parish E2E tests.
 *
 * Starts the worktree-isolated managed launcher, which rebuilds and snapshots
 * the UI before compiling the axum server, then runs Chromium against it.
 */
export default defineConfig({
	testDir: 'e2e',
	outputDir: 'e2e/test-results',
	snapshotPathTemplate:
		'{testDir}/screenshots/baseline/{platform}/{testName}/{arg}{ext}',
	fullyParallel: false,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	workers: 1,
	reporter: 'html',
	timeout: 60_000,
	expect: {
		toHaveScreenshot: {
			maxDiffPixelRatio: 0.01,
		},
	},

	use: {
		baseURL: `http://localhost:${testPort}`,
		viewport: { width: 1280, height: 800 },
		trace: 'on-first-retry',
		screenshot: 'only-on-failure',
	},

	projects: [
		{
			name: 'chromium',
			use: { ...devices['Desktop Chrome'] },
		},
	],

	// Never reuse an arbitrary listener on this port: the helper must own the
	// server process whose worktree/CSP coherence this run relies on.
	webServer: playwrightWebServerConfig(testPort),
});
