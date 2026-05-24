import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright configuration for Parish E2E tests.
 *
 * Starts the axum web server via `cargo run -- --web <port>` and runs
 * Chromium tests against it.
 */
export default defineConfig({
	testDir: 'e2e',
	outputDir: 'e2e/test-results',
	snapshotPathTemplate: '{testDir}/screenshots/baseline/{testName}/{arg}{ext}',
	fullyParallel: false,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 1 : 0,
	workers: 1,
	reporter: 'html',
	timeout: 60_000,
	expect: {
		toHaveScreenshot: {
			maxDiffPixelRatio: 0.01
		}
	},

	use: {
		baseURL: `http://localhost:${process.env.PARISH_TEST_PORT || 3099}`,
		viewport: { width: 1280, height: 800 },
		trace: 'on-first-retry',
		screenshot: 'only-on-failure'
	},

	projects: [
		{
			name: 'chromium',
			use: { ...devices['Desktop Chrome'] }
		}
	],

	webServer: {
		command: `cd ../.. && cargo run -p parish-repl -- --web ${process.env.PARISH_TEST_PORT || 3099}`,
		url: `http://localhost:${process.env.PARISH_TEST_PORT || 3099}/api/world-snapshot`,
		timeout: 120_000, // cargo build can be slow on first run
		reuseExistingServer: !process.env.CI,
		// Each Playwright test gets a fresh browser context = fresh
		// session on the parish web server. With the default cap of 50
		// and the suite now sitting at ~54 tests, the last few tests
		// hit "Server at capacity (50/50 sessions)" before reaching the
		// app shell. Bump the cap for the e2e harness; admission
		// control still gets its own dedicated coverage in
		// parish-server/tests/admission_control.rs.
		env: {
			PARISH_MAX_SESSIONS: '500'
		}
	}
});
