# limerick-tauri — agent scope

Desktop entry point + MCP bridge. Thin adapter — gameplay logic lives in `limerick-core`. See root [`AGENTS.md`](../../../AGENTS.md) and [`mcp_bridge.rs`](src/mcp_bridge.rs).

## Scoped commands

```sh
cargo run  -p limerick-tauri -- --mcp-port 3030    # live desktop window + MCP
cargo test -p limerick-tauri                       # unit
just run                                          # cargo tauri dev (full UI loop)
```

## Local gotchas

- **MCP bridge expects backend on 127.0.0.1:3030.** When starting the desktop, the `--mcp-port` flag also opens the bridge — `mcp__limerick__*` tools speak to it via HTTP. Without `--mcp-port`, MCP tools error with "transport error". **Driving the game via MCP** (time control via `/pause`+`/resume`, async-world event deltas, screenshot/focus gotchas, `submit_input` returns `null`): see [`docs/agent/driving-the-game-via-mcp.md`](../../../docs/agent/driving-the-game-via-mcp.md).
- **Tauri IPC types must match TS** (`limerick/apps/ui/src/lib/types.ts`). serde uses snake_case — drift breaks the frontend silently.
- **[Cross-runtime orchestration](../../../docs/agent/engineering-rules.md#cross-runtime-orchestration) belongs in `limerick-core`.** Do not duplicate handlers from `limerick-server`. Wire via `EventEmitter`.
- **`commands/` and `editor_commands/` are typed adapters only.** Real work delegates into `limerick-core`; this layer only marshalls + emits events.
- **Onboarding wizard / BYOK setup** lives here (`limerick_setup_status`, `limerick_setup_byok`); state mutations persist to keychain + `limerick.toml`.
- **Demo profiling must isolate `LIMERICK_USER_CONFIG_DIR`.** Tauri reapplies saved wizard/category overrides from the user config dir after base env resolution, which can bypass a proxy unless the profiling run points config/data/saves at a temp directory.
- **Tauri demo reads per-category routing from user config, not category env vars.** `LIMERICK_INTENT_MODEL` works for the CLI config path, but Tauri startup hydrates category overrides from `LIMERICK_USER_CONFIG_DIR/limerick.toml`.
- **Desktop screenshots capture native window pixels (xcap), NOT html-to-image, because the MapLibre minimap is a WebGL canvas that html-to-image cannot read.** The gameplay map draws cross-origin tiles, which taint the WebGL canvas so `canvas.toDataURL()` is blocked — the map serializes blank no matter what (`preserveDrawingBuffer` does NOT fix it). `do_take_screenshot` (limerick-tauri) now captures the real window via `xcap` and only falls back to the frontend `html-to-image` round-trip when native capture is unavailable (no window / no macOS Screen-Recording permission / headless). Native capture needs the desktop window present + foreground; if a `limerick_take_screenshot` comes back with a blank map, check the Tauri log for a `native screenshot failed` WARN (it fell back). Web mode (`limerick-server`) has no window and still uses the browser-supplied html-to-image data URL. See #1160.
- **Desktop screenshot MCP captures must always be fresh.** Blank/degenerate frames and unavailable graphical surfaces are hard errors; never substitute a previously saved image. The static graphical launcher waits for the UI's explicit first-frame readiness token, then the screenshot request requires a fast frontend receipt before its longer rasterization deadline.
- **An API key alone does not select Google in an isolated Tauri demo profile.** When a proof deliberately uses fresh config/data/save roots, set the provider, model, and base URL explicitly as well as `GOOGLE_API_KEY`; otherwise onboarding can see the credential while the isolated runtime still has no cloud client selected.
- **A clean-profile Tauri launch on `main` panics until #1938 is fixed.** `spawn_catalog_refresh_v2` calls `tokio::spawn` before Tauri's runtime exists ("there is no reactor running"), so an isolated `LIMERICK_USER_DATA_DIR` run never reaches the bridge. The Tauri debug window also needs the Vite dev server on `devUrl` (:5173) and `LIMERICK_MODS_DIR` for the mod, or screenshots report the UI not ready.

## Module map

`commands/` runtime IPC, `editor_commands/` Designer IPC, `events/` emission, `mcp_bridge.rs` HTTP→backend, `main`+`lib` startup wiring.
