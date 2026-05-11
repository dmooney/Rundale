# Proof: screenshot capture (player-triggered, MCP-readable)

Evidence type: gameplay transcript

## What changed

Implements the screenshot-capture feature scoped under "Future work →
Screenshot capture" in `parish/crates/parish-mcp/README.md`.

- Frontend: `parish/apps/ui/src/lib/screenshot.ts` exposes
  `captureScreen()`, which uses `html-to-image` to render the live
  `.app-shell` (or `document.body` fallback) to a PNG data URL.
- IPC wrapper: `saveScreenshot(dataUrl)` and `getLatestScreenshot()` in
  `parish/apps/ui/src/lib/ipc.ts` use the existing `command()` helper so
  the call works in both Tauri desktop mode and web mode.
- Keybinding + toast: `+page.svelte` binds **F2** to capture and shows a
  transient bottom-centre toast with the saved path (or a failure
  message). Listed alongside the existing F5/F11/F12 chord.
- Backend (Tauri): `save_screenshot(data_url)` decodes the base64 PNG,
  writes it to `<saves_dir>/screenshots/parish-<ISO-timestamp>.png`, and
  caches the absolute path on a new `AppState::latest_screenshot_path`
  field. `get_latest_screenshot()` reads the cached path back, re-stating
  the file each call so a deleted screenshot is reported as missing.
- MCP bridge: `GET /api/latest-screenshot` returns the cached metadata
  (`{path, taken_at, size_bytes}`) so an MCP client can read the path
  and load the file via its own filesystem tool.
- Server stubs: `parish-server` registers `POST /api/save-screenshot`
  and `GET /api/latest-screenshot` returning 501 (same Tauri-only
  pattern as the demo routes), keeping `wiring_parity` green.
- Registries: command_registry and route_registry updated to keep the
  parity sensor satisfied.
- MCP tool: `parish_latest_screenshot` (empty-input schema) translates
  to `get_latest_screenshot` and returns `{path, taken_at, size_bytes}`
  or `null`.

The path-only (no inline image) and player-trigger-only design choices
match the README's two open design questions; the rationale is
preserved there for future implementers.

## Behavior walkthrough

1. The player presses **F2** in the live desktop window.
2. The frontend handler `handleScreenshot()` calls `captureScreen()`,
   which `html-to-image.toPng(.app-shell, {cacheBust: true,
   pixelRatio: devicePixelRatio})` returns as
   `data:image/png;base64,...`.
3. `saveScreenshot(dataUrl)` invokes the Tauri command, passing
   `dataUrl` (auto-converted to `data_url` by the Tauri IPC bridge).
4. `do_save_screenshot` decodes the base64, writes the PNG under
   `<saves_dir>/screenshots/parish-2026-05-09T21-04-15Z.png`, and
   stores the absolute path on `AppState::latest_screenshot_path`.
5. `ScreenshotInfo {path, taken_at, size_bytes}` returns to the
   frontend, which flashes a 2.5s toast: `Screenshot saved: <path>`.
6. An MCP client invokes the `parish_latest_screenshot` tool with
   no arguments. The MCP server translates that to
   `get_latest_screenshot` → `GET /api/latest-screenshot` → the bridge
   handler reads `AppState::latest_screenshot_path`, stats the file,
   and returns the same `ScreenshotInfo` to the model.

## Test results

```
cargo test -p parish-tauri:    51 passed (incl. 6 new screenshot tests)
cargo test -p parish-mcp:      35 passed (incl. 2 new tool tests)
cargo test -p parish-server:  180 passed
cargo test -p parish-core:    400 passed (wiring_parity green)
cargo test --workspace:       all green
npx vitest run:               399 passed (incl. 3 new screenshot.test.ts)
cargo clippy -p parish-tauri -p parish-mcp -p parish-server --all-targets -- -D warnings: clean
cargo fmt --all:              clean
svelte-check:                 0 errors (1 pre-existing warning unrelated)
```

New Rust tests (`parish-tauri/src/commands.rs::cmd_tests`):
- `decode_data_url_png_accepts_well_formed_url`
- `decode_data_url_png_rejects_wrong_prefix`
- `decode_data_url_png_rejects_invalid_base64`
- `write_screenshot_to_disk_creates_file_and_reports_size`
- `do_save_screenshot_round_trips_through_app_state`
- `do_get_latest_screenshot_returns_none_when_unset`
- `do_get_latest_screenshot_returns_none_when_file_missing`

New Rust tests (`parish-mcp/src/tools.rs::tests`):
- `latest_screenshot_takes_no_args_and_routes_to_get`
- `registry_includes_latest_screenshot_tool`

New mcp-bridge route table assertion: `/api/latest-screenshot` is now
in `EXPECTED` and the canonical-translation loop covers
`get_latest_screenshot`.

New frontend tests (`parish/apps/ui/src/lib/screenshot.test.ts`):
- Targets `.app-shell` when present.
- Falls back to `document.body` when `.app-shell` is absent.
- Forwards `cacheBust` and a sensible `pixelRatio` to `html-to-image`.

The pre-existing `command_registry` count assertion was updated
(32 → 34) and the imports list now includes the two new commands so
that future renames break the test at compile time.

## Architecture fitness

`cargo test -p parish-core --test architecture_fitness` is still green:
the new code lives in `parish-tauri` (already runtime-coupled) and the
new MCP tool lives in `parish-mcp` (already protocol-coupled). No
backend-agnostic crate gained a forbidden dependency. The wiring
parity sensor is satisfied because both `EXPECTED_COMMANDS` and
`EXPECTED_HTTP_ROUTES` were updated together.

## Mode parity

Per CLAUDE.md rule #2, the new IPC surface ships in both backends:
- Tauri: `save_screenshot` + `get_latest_screenshot` registered in
  `tauri::generate_handler!`.
- HTTP server: `POST /api/save-screenshot` + `GET /api/latest-screenshot`
  registered with 501 stubs (mirrors the existing demo routes).

The `parish-mcp` client therefore behaves identically against either
backend except that the headless server returns 501 — by design, since
there is no DOM in a headless context to capture.

## Scope notes

- MCP-trigger (model asks the live window to capture) is intentionally
  deferred. The README's "Two open design questions" section covers the
  oneshot/event-roundtrip approach for when this becomes desirable.
- Inline image-part responses in the MCP envelope are also deferred.
  The model can `Read` the file via its filesystem tool today.
