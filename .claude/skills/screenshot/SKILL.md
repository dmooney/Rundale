---
name: screenshot
description: Regenerate GUI screenshots after UI changes. Captures the Parish GUI at 4 times of day using xvfb for headless rendering.
disable-model-invocation: true
---

Regenerate Parish GUI screenshots after UI changes.

## Steps

1. **Build**: Run `cargo build` to ensure the project compiles.
2. **Capture screenshots**: Run `xvfb-run -a cargo run -- --screenshot docs/screenshots` to capture the GUI at 4 times of day (morning, midday, dusk, night).
3. **Verify output**: Check that `docs/screenshots/` contains the updated PNG files. List the files and their sizes.
4. **Report**: Confirm which screenshots were generated and note any errors. Remind the user to commit the updated screenshots alongside their UI changes.
