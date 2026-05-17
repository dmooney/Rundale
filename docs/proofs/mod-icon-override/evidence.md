Evidence type: screenshot

The proof image `rundale-icon-64.png` is a 64px downsample from the mod-owned
Rundale app icon set. It exercises the small-size readability target for the
runtime icon override path.

Follow-up desktop verification: the Tauri setup path now applies the same
mod-owned PNG to macOS's native application icon via AppKit before applying it
to individual windows, so the Dock surface is covered as well as the window
surface.

Checks run:

- `cargo test -p parish-core game_mod::tests::test_mod_icon_paths`
- `npm run test -- app-icon.test.ts`
- `cargo check -p parish-tauri`
- `just notices`
- `just agent-check`
- `git diff --check`
- `just check`
