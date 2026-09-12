# Implementation plan — limerick-mcp-cold-register

One commit per step, conventional-commit prefixes. Order chosen so each step is
independently testable.

## Step 1 — `chore(mcp): add limerick-mcp to workspace default-members`

- `limerick/Cargo.toml`: `default-members = ["crates/limerick-engine", "crates/limerick-mcp"]`.
- Verify: `cargo clean -p limerick-mcp` (NOT global — shared target, see memory), then
  `just build`, assert `limerick/target/debug/limerick-mcp` exists.
- Test: covers AC #4.

## Step 2 — `refactor(mcp): make manifest.json the source of truth`

- Author `limerick/crates/limerick-mcp/manifest.json` from the current `registry()`:
  `{ "protocolVersion": "2025-06-18", "serverInfo": {"name": "limerick-mcp"},
"tools": [ {name, description, inputSchema} ... ] }`. Hand-port the existing inline
  `json!` schemas verbatim so the wire shape is byte-identical to today.
- `tools.rs`:
  - `ToolDef` fields become owned: `name: String, description: String,
input_schema: Value`, keeping `translate: fn(&Value) -> Result<(String, Value), String>`.
  - `static MANIFEST_JSON: &str = include_str!("../manifest.json");` parsed once into a
    `OnceLock<ParsedManifest>` (`protocolVersion`, `serverInfo.name`, `tools`).
  - `fn translate_for(name: &str) -> Option<fn(&Value) -> Result<(String, Value), String>>`
    — the name→fn table (every existing `translate_*`).
  - `registry()` iterates parsed manifest tools, pairs each with `translate_for(name)`,
    builds owned `ToolDef`s. Missing fn → skip + the bijection test will fail (don't
    silently drop).
  - `descriptor_json()` unchanged in shape (now from owned fields).
- `mcp.rs`: `initialize` reads `protocolVersion` + `serverInfo.name` from the parsed
  manifest; version stays `CARGO_PKG_VERSION`. Drop the now-unused `PROTOCOL_VERSION` /
  `SERVER_NAME` consts if fully superseded (keep `SERVER_VERSION`).
- Existing unit tests in `mcp.rs` (tools_list_includes_curated_set, etc.) must still pass
  unchanged — they assert on tool names, which now come from the manifest.
- No `--dump-manifest`, no clap change.
- Covers AC #5.

## Step 3 — `test(mcp): name↔translate bijection`

- `limerick/crates/limerick-mcp/tests/manifest_bijection.rs`: `manifest_translate_bijection`
  — parse committed `manifest.json`; assert every tool name resolves `translate_for`,
  and every name in the translate table appears in the manifest (no orphans either way).
- Confirm: temporarily add a manifest tool with no fn → test fails; remove → green.
- Covers AC #6.

## Step 4 — `feat(mcp): no-build cold-start stdio shim`

- New `limerick/scripts/limerick-mcp-cold-shim.py` (stdlib only):
  - args `--manifest PATH --bin PATH`.
  - Loop reading newline-delimited JSON-RPC from stdin.
  - `initialize` → result from manifest (protocolVersion, serverInfo, tools capability).
  - `tools/list` → `{"tools": manifest["tools"]}`.
  - `ping` → `{}`; `notifications/initialized`/`initialized` → no response (notification).
  - `tools/call` → if `--bin` now executable, enter **proxy mode**: `subprocess.Popen`
    the real binary with the same argv tail, replay the captured `initialize` +
    `initialized`, forward the current request, then pump stdin↔child.stdin and
    child.stdout↔stdout for the rest of the process life. Else respond with a
    `tools/call` success envelope `{"content":[{"type":"text","text":"limerick engine is
still building — retry shortly"}],"isError":true}`.
  - Unknown method → JSON-RPC error -32601.
  - Never spawn cargo.
- Keep it small and dependency-free; flush stdout per line.

## Step 5 — `feat(mcp): launcher uses cold shim instead of synchronous build`

- `limerick/scripts/limerick-mcp-launch.sh`: remove the synchronous `cargo build` block.
  If `$MCP_BIN` executable → `exec "$MCP_BIN" "$@"`. Else →
  `exec python3 "$SCRIPT_DIR/limerick-mcp-cold-shim.py" --manifest <repo>/limerick/crates/limerick-mcp/manifest.json --bin "$MCP_BIN"`.
- Keep the JSON-RPC-on-stdout contract (all chatter to stderr).
- Covers AC #1, #2, #3.

## Step 6 — `test(mcp): cold-register verification fixture`

- `limerick/testing/fixtures/mcp_cold_register.sh`:
  - Scenario A (cold, no backend): point launcher at a non-existent `MCP_BIN` (temp
    `CARGO_TARGET_DIR` or a `--bin` override), ensure :3030 unused; pipe `initialize` +
    `tools/list`; assert serverInfo + tool-name set == `manifest.json`; assert no cargo
    spawned. (AC #1)
  - Scenario B (cold, tools/call): same, send `tools/call limerick_world_snapshot`; assert
    `isError:true` + "building" text. (AC #2)
  - Scenario C (warm): with the real built binary + a live :3030 (reuse the running
    Tauri on this machine, or skip the live-call assert if absent); assert direct exec +
    real `isError:false` data. (AC #3)
  - Print PASS/FAIL per criterion.
- Capture to `.proofs/limerick-mcp-cold-register/transcript.txt`.

## Step 7 — `just check` + proof bundle

- `just check` (fmt + clippy + tests). (AC #7)
- Write `evidence.md` (Evidence type: gameplay transcript form N/A — use the non-live
  test-transcript form; limerick-mcp is not in the live-proof path list) mapping each
  criterion to fixture / test output lines.
- Write `judge.md` (Verdict: sufficient / Technical debt: clear / Acceptance criteria: met).
- `just agent-check`, then `just attach-proof limerick-mcp-cold-register` after the PR is up.

## Risks / notes

- **Shared cargo target** (memory): never `cargo clean` globally; use `-p limerick-mcp`.
- Proxy handoff is the trickiest bit — must replay `initialize`/`initialized` so the
  real binary's protocol state matches what the client already negotiated. Keep the
  captured init params verbatim. If proxy proves fragile, the fallback degrades to AC #2
  (return "retry" until next session), which still unblocks registration — but proxy is
  the goal for no-restart recovery.
- `MCP_TIMEOUT`: not relied upon; the fix removes build from the init path entirely.
