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

Transparent-corner follow-up: the Rundale mod icon set now ships as RGBA PNGs
with an antialiased rounded-rectangle alpha mask. `sips -g hasAlpha` reports
`hasAlpha: yes` for `icon-512.png`, `icon-64.png`, and the proof thumbnail.
A PNG alpha decode check confirmed all four corner pixels are alpha `0` and
the center pixel remains alpha `255` for representative sizes.

Acceptance criteria mapping:

- Mod ownership and engine icon separation: Rundale icons are present under
  `mods/rundale/assets/icons/app/`; the Parish engine icon file was not changed.
- Branding declaration and path safety: `mods/rundale/ui.toml` contains the
  `[branding]` icon paths, and
  `cargo test -p parish-core game_mod::tests::test_mod_icon_paths` covers both
  valid asset resolution and escaping-path rejection.
- Web/browser surface: `app-icon.test.ts` verifies active-mod favicon and
  apple-touch icon link updates.
- Desktop surface: `cargo check -p parish-tauri` validates the Tauri window,
  macOS Dock, and `RunEvent::Ready` icon paths; `just run` launched the dev path
  that uses the Ready-event reapply.
- Transparent corners: `sips -g hasAlpha` and a PNG alpha decode spot-check
  verified RGBA output, alpha `0` corners, and alpha `255` centers.
- Full gate: `just agent-check`, `git diff --check`, and `just check` passed
  after the implementation and asset updates.

Checks run:

- `cargo test -p parish-core game_mod::tests::test_mod_icon_paths`
- `npm run test -- app-icon.test.ts`
- `cargo check -p parish-tauri`
- `just run` (launched and interrupted after confirming the dev path reaches the app)
- `sips -g hasAlpha mods/rundale/assets/icons/app/icon-512.png mods/rundale/assets/icons/app/icon-64.png docs/proofs/mod-icon-override/rundale-icon-64.png`
- PNG alpha decode spot-check for `icon-16.png`, `icon-64.png`, `icon-512.png`, `icon-1024.png`, and `favicon-32.png`
- `just notices`
- `just agent-check`
- `git diff --check`
- `just check`
