Verdict: sufficient
Technical debt: clear

The PR introduces `parish_take_screenshot`, a new MCP tool that completes the
screenshot round-trip without requiring a player at the keyboard.

Evidence review:
- The `do_take_screenshot` shared helper eliminates the code duplication the
  Gemini reviewer flagged, and plugs the memory-leak path where `app.emit`
  failure previously left a stale sender in the map.
- The oneshot channel now carries `Result<ScreenshotInfo, String>`, so frontend
  capture failures (html-to-image errors, IPC errors) are reported immediately
  via `notify_screenshot_error` rather than waiting for the 15-second timeout.
- Wiring parity is maintained: `take_screenshot` is registered in both
  `command_registry.rs` and `parish-server`'s route registry (501 stub).
- All 43 MCP tool tests pass, including two new tests for the new tool.
- The command count test updated to 35, command names are unique and well-formed.
- No placeholder panics, TODO stubs, or incomplete branches found.
- The feature is Tauri-mode only by design; headless mode returns 501 with a
  clear error message — this is consistent with the existing screenshot surface
  and is not debt.
