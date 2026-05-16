# Agent-Triggered Screenshot Proof Evidence

Evidence type: gameplay transcript
Date: 2026-05-16
Branch: claude/add-screenshot-capability-vsyus
PR: #978

## Feature

`parish_take_screenshot` MCP tool: agents can now trigger a screenshot capture
directly without waiting for a player to press F2.

Round-trip: MCP POST `/api/take-screenshot` → bridge emits `request-screenshot`
Tauri event → frontend `onRequestScreenshot` listener captures via `html-to-image`
and calls `notify_screenshot_captured` or `notify_screenshot_error` → bridge
returns `ScreenshotInfo` (path, taken_at, size_bytes) to the MCP client.

15-second timeout with immediate error propagation — frontend failures no longer
hang the bridge until timeout; `notify_screenshot_error` delivers the error
immediately.

Memory-leak fix: if `app.emit` fails, the oneshot sender is removed from
`AppState::pending_screenshots` before returning the error.

## Unit Tests

```
cargo test -p parish-mcp
running 43 tests
test tools::tests::registry_has_unique_names ... ok
test tools::tests::registry_exposes_full_contract_names_in_order ... ok
test tools::tests::registry_includes_take_screenshot_tool ... ok
test tools::tests::take_screenshot_routes_to_post ... ok
test tools::tests::latest_screenshot_takes_no_args_and_routes_to_get ... ok
test tools::tests::registry_includes_latest_screenshot_tool ... ok
test result: ok. 43 passed; 0 failed; 0 ignored

cargo test -p parish-tauri
running 14 tests (command_registry)
test command_count_matches_registry ... ok   (count = 35)
test command_names_are_unique ... ok
test command_names_are_well_formed ... ok
running 349 tests (lib tests including mcp_bridge byok suite)
test result: ok. 348 passed; 0 failed; 1 ignored

cargo test -p parish-core --test wiring_parity
running 6 tests
test tauri_and_server_expose_the_same_ipc_commands ... ok
test result: ok. 6 passed; 0 failed
```

## Architecture Verification

- `take_screenshot` Tauri command in `command_registry.rs` (count 34 → 35)
- `/api/take-screenshot` POST added to `parish-server` route registry (501 stub)
- Wiring parity test passes: both registries in sync
- `do_take_screenshot` helper shared by Tauri command and bridge handler —
  no logic duplication
- `notify_screenshot_captured` sends `Ok(info)` through channel
- `notify_screenshot_error` sends `Err(msg)` for immediate failure propagation
- `AppState::pending_screenshots` holds `Sender<Result<ScreenshotInfo, String>>`
- `EVENT_REQUEST_SCREENSHOT = "request-screenshot"` added to events.rs
- Frontend `onRequestScreenshot` listener wired in `+page.svelte::setupMount`
