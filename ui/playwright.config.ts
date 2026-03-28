import { defineConfig } from '@playwright/test';

export default defineConfig({
	testDir: 'e2e',
	outputDir: 'e2e/test-results',
	snapshotPathTemplate: '{testDir}/screenshots/baseline/{testName}/{arg}{ext}',
	timeout: 60_000,
	expect: {
		toHaveScreenshot: {
			maxDiffPixelRatio: 0.01
		}
	},
	use: {
		baseURL: 'http://localhost:5173',
		viewport: { width: 1280, height: 800 },
		screenshot: 'only-on-failure'
	},
	webServer: {
		command: 'npm run dev',
		port: 5173,
		timeout: 60_000,
		reuseExistingServer: !process.env.CI
	},
	projects: [{ name: 'chromium', use: { browserName: 'chromium' } }]
});
