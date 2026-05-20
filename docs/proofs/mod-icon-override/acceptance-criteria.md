# Acceptance Criteria: mod-icon-override

## Task

Rundale should own its decorative "R" app icon as mod content, while the Parish
engine keeps its default icon. When the Rundale mod is active, the mod icon
should override the desktop and browser-facing app icon surfaces without
requiring the base engine icon to change. The icon artwork must remain legible
at small app-icon sizes and must have real transparent rounded corners rather
than painted opaque corners.

## Criteria

- The Parish engine icon remains untouched, and the Rundale icon files live
  under `mods/rundale/assets/icons/app/` as mod-owned assets.
- `mods/rundale/ui.toml` declares `[branding]` values for the app icon and
  favicon, and those paths resolve under the mod's `assets/` directory.
- Mod branding path validation rejects attempts to escape the mod asset
  directory, so a malicious or malformed mod cannot point the app icon at an
  arbitrary local file.
- The web runtime exposes the active mod's configured app icon and favicon
  through stable URLs, and the Svelte frontend applies those URLs to browser
  favicon and apple-touch-icon links.
- The Tauri runtime applies the active mod icon to desktop windows and to the
  macOS Dock/application icon. In dev mode, `cargo tauri dev` / `just run`
  reapplies the mod icon after Tauri's own `RunEvent::Ready` bundle-icon pass.
- Every shipped Rundale app-icon PNG is RGBA and has transparent rounded
  corners: representative corner pixels are alpha `0`, while the center remains
  alpha `255`.
- The changed behavior is covered by focused tests or live proof evidence:
  core mod-path tests, frontend favicon tests, Tauri compile checks, a `just run`
  launch of the dev path, alpha-channel validation, and the full `just check`
  gate.

## Verification

```sh
cargo test -p parish-core game_mod::tests::test_mod_icon_paths
./node_modules/.bin/vitest run src/lib/app-icon.test.ts
cargo check -p parish-server -p parish-tauri
just run
sips -g hasAlpha mods/rundale/assets/icons/app/icon-512.png \
  mods/rundale/assets/icons/app/icon-64.png \
  docs/proofs/mod-icon-override/rundale-icon-64.png
just agent-check
git diff --check
just check
```

Expected signals:

- Core tests prove icon branding paths resolve under `assets/` and reject
  escaping paths.
- Frontend tests prove the active mod icon updates favicon and apple-touch icon
  links.
- Tauri/server checks prove all icon-serving and desktop-application paths
  compile.
- `just run` starts the Tauri dev path, which exercises the Ready-event path
  used to reapply the mod Dock icon after Tauri's dev-mode bundle icon.
- `sips` reports `hasAlpha: yes`, and the PNG alpha spot-check records
  transparent corner pixels with an opaque center.
- `just check` remains green.
