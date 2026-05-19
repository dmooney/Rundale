Evidence type: screenshot

The proof image `rundale-icon-64.png` is a 64px downsample from the mod-owned
Rundale app icon set. It exercises the small-size readability target for the
runtime icon override path.

Follow-up desktop verification: the Tauri setup path now applies the same
mod-owned PNG to macOS's native application icon via AppKit before applying it
to individual windows, so the Dock surface is covered as well as the window
surface.

Second dev-mode follow-up: `cargo tauri dev`, which is what `just run` uses,
reapplies Tauri's configured bundle icon during the `RunEvent::Ready` pass on
macOS. The desktop runtime now reapplies the active mod icon from the app's
`Ready` callback after Tauri's own dev icon update, keeping `just run` aligned
with packaged launches.

Checks run:

- `cargo test -p parish-core game_mod::tests::test_mod_icon_paths`
- `npm run test -- app-icon.test.ts`
- `cargo check -p parish-tauri`
- `just run` (launched and interrupted after confirming the dev path reaches the app)
- `just notices`
- `just agent-check`
- `git diff --check`
- `just check`
