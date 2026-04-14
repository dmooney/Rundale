---
name: screenshot
description: Regenerate GUI screenshots via Playwright. Captures the Rundale GUI at 4 times of day (morning, midday, dusk, night) using headless Chromium — no X11/GDK required.
disable-model-invocation: true
---

Regenerate Rundale GUI screenshots after UI changes.

## Steps

1. **Install deps** (if needed): Run `cd apps/ui && npm install` to ensure Playwright is installed.
2. **Capture screenshots**: Run `cd apps/ui && npx playwright test e2e/screenshots.spec.ts` to capture the GUI at 4 times of day (morning, midday, dusk, night) using headless Chromium with mocked Tauri IPC.
3. **Verify output**: Check that `docs/screenshots/` contains the updated PNG files. List the files and their sizes.
4. **Report**: Confirm which screenshots were generated and note any errors. Remind the user to commit the updated screenshots alongside their UI changes.

## Notes

- Screenshots are captured via Playwright against the Vite dev server with Tauri IPC mocked.
- No `xvfb` or GDK/GTK dependencies required — runs on any platform with Chromium.
- To update visual regression baselines: `cd apps/ui && npx playwright test e2e/screenshots.spec.ts --update-snapshots`
- Full E2E test suite: `cd apps/ui && npx playwright test` or `just ui-e2e`
