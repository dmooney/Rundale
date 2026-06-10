# Design — parish-mcp-cold-register

## Problem

On a fresh worktree, the project `parish` MCP server fails to register
(`claude mcp list` → `parish: ✘ Failed to connect`), so none of the
`mcp__parish__*` tools exist for the whole session, and there is **no in-session
reload** (#1352). The quality-harness, demo-audit-mcp, and any MCP-driven QA are
blocked until the user kills the session and starts a new one.

### Root cause (Five Whys)

1. Why no tools? — The `parish` MCP server was marked `Failed to connect` at init.
2. Why failed? — The stdio process Claude spawned (`parish-mcp-launch.sh`) did not
   complete the `initialize` handshake within Claude's MCP startup window.
3. Why not in time? — On a cold worktree the launcher's `MCP_BIN` is missing, so the
   script runs `cargo build -p parish-mcp` **synchronously** before `exec`-ing the
   binary; a cold compile takes far longer than the init window.
4. Why does building block registration? — Because the registering process _is_ the
   Rust binary, and it cannot exist until compiled. The background
   `SessionStart--build-mcp.sh` races the launcher and loses.
5. Why is a build even on the critical path? — Two coupled assumptions:
   (a) the only thing that can answer `tools/list` is the compiled binary; and
   (b) the binary is not part of the normal build (`default-members =
["crates/parish-engine"]`), so nothing reliably has it built before init.

Note: the backend being down is **not** a cause. `McpServer::new` builds
`registry()` with no I/O, and `initialize`/`tools/list` never touch the backend
(`parish/crates/parish-mcp/src/mcp.rs`). A dead :3030 only shows up as
`isError:true` on a `tools/call` — proven by `backend_transport_error_is_successful_tool_error`.

## What the player/operator experiences

After the fix: open a Claude Code session in any worktree — fresh or warm, backend
up or down — and the `mcp__parish__*` tools are present immediately. The harness can
start. If the operator invokes a parish tool before the engine is built/started, they
get a readable "engine still building / not started" message, not a stuck session.
Once the normal build finishes (or they start the game), tool calls begin returning
real data with no session restart.

## Affected components

- `parish/Cargo.toml` — `default-members` gains `crates/parish-mcp`.
- `parish/crates/parish-mcp/manifest.json` — **new**, committed; the single source of
  truth read by both Rust (`include_str!`) and the shim.
- `parish/crates/parish-mcp/src/tools.rs` — `ToolDef` fields become owned; `registry()`
  builds from the parsed manifest, pairing each tool name with a `translate` fn.
- `parish/crates/parish-mcp/src/mcp.rs` — `initialize` reads `protocolVersion` +
  `serverInfo.name` from the parsed manifest (version from `CARGO_PKG_VERSION`).
- `parish/scripts/parish-mcp-launch.sh` — branch: binary present → `exec` real binary;
  absent → `exec` the no-build shim.
- `parish/scripts/parish-mcp-cold-shim.py` — **new**, no-build python3 stdio shim.
- `parish/crates/parish-mcp/tests/` — `manifest_translate_bijection` test.
- `parish/testing/fixtures/mcp_cold_register.sh` — **new** verification fixture.

## Design

### 1. parish-mcp in default-members

`default-members = ["crates/parish-engine", "crates/parish-mcp"]`. Now `cargo build`,
`just build`, and the SessionStart background build all produce the binary as part of
the normal build — no parish-mcp-specific build step (per reviewer: "built as part of
the rest of the build"). Cost: a plain default build also compiles parish-mcp (small
leaf crate; reqwest/clap already in the graph elsewhere). Acceptable.

### 2. No-build cold shim

`parish-mcp-launch.sh` logic:

```
MCP_BIN="$TARGET_DIR/debug/parish-mcp"
if [ -x "$MCP_BIN" ]; then
    exec "$MCP_BIN" "$@"                      # warm fast path — unchanged
fi
exec python3 "$SCRIPT_DIR/parish-mcp-cold-shim.py" \
        --manifest "$REPO/parish/crates/parish-mcp/manifest.json" \
        --bin "$MCP_BIN"                      # cold path — instant register
```

The launcher no longer builds. The shim (`parish-mcp-cold-shim.py`, stdlib only, no
deps):

- Reads `manifest.json` once at startup.
- Serves over stdio JSON-RPC: `initialize` (returns `manifest.protocolVersion` +
  `manifest.serverInfo` + `{"capabilities":{"tools":{"listChanged":false}}}`),
  `tools/list` (returns `manifest.tools`), `ping`, and silently accepts
  `notifications/initialized`.
- On `tools/call`: re-check `--bin` on disk. If it now exists, hand off — simplest
  correct form: spawn the real binary, replay the client's `initialize` +
  `notifications/initialized`, forward this and all subsequent traffic
  bidirectionally (the shim becomes a transparent stdio proxy for the rest of the
  session). If it still does not exist, return a JSON-RPC **success** envelope with
  `isError:true` and text "parish engine is still building — retry shortly".
- The shim never spawns `cargo`.

Why python3, not bash: line-delimited JSON-RPC with bidirectional proxy is impractical
in bash; python3 is present on macOS and the remote sandbox image. Why not register
statically inside the Rust binary only: the binary cannot exist on a cold worktree —
the no-build layer is the whole point.

Proxy detail: MCP stdio is newline-delimited JSON-RPC. The shim reads requests line by
line; once in proxy mode it pumps bytes both directions and stops interpreting them.
Because `tools/list` is identical between shim and binary (both read the same
`manifest.json`), the client never sees a tool-set change across the handoff
(`listChanged:false` stays honest).

### 3. Manifest as single source of truth (inverted)

`parish/crates/parish-mcp/manifest.json` is authoritative. Shape:

```json
{
  "protocolVersion": "2025-06-18",
  "serverInfo": { "name": "parish-mcp" },
  "tools": [{ "name": "...", "description": "...", "inputSchema": {} }]
}
```

`serverInfo.version` is deliberately omitted — it would otherwise be a hand-edited copy
of `CARGO_PKG_VERSION`. The Rust server injects `CARGO_PKG_VERSION` at runtime; the shim
omits version. Clients tolerate a missing version.

Consumers:

- Rust (`tools.rs`): `static MANIFEST_JSON: &str = include_str!("../manifest.json");`
  parsed once into a `OnceLock<ParsedManifest>`. `ToolDef` changes from `&'static str`
  fields to owned `name: String, description: String, input_schema: Value`, keeping the
  existing `translate: fn(&Value) -> Result<(String, Value), String>`. `registry()`
  iterates the parsed manifest tools and pairs each with its `translate` fn via a
  name→fn table (`fn translate_for(name: &str) -> Option<TranslateFn>`). A manifest tool
  with no translate fn is a hard error surfaced by the bijection test. `mcp::initialize`
  reads `protocolVersion` + `serverInfo.name` from the parsed manifest (+
  `CARGO_PKG_VERSION` for version).
- Shim (`parish-mcp-cold-shim.py`): reads the same file, serves it verbatim.

Schema drift is impossible — exactly one definition of each tool's wire shape, read by
both the Rust binary and the no-build shim. The only thing that can diverge is the
name↔translate pairing, guarded by a test:

- `manifest_translate_bijection`: every manifest tool name resolves a `translate` fn,
  and every name in the translate table appears in the manifest. Fails CI otherwise.

No `--dump-manifest`, no generated artifact, no `BLESS` step. Changing the tool set =
editing `manifest.json` (wire shape) and, for a genuinely new tool, adding its
`translate` fn — the bijection test enforces both halves were done.

#### Alternative considered (not chosen)

Keep `registry()` as the Rust source of truth and emit/diff a generated `manifest.json`
via `parish-mcp --dump-manifest` + a drift test. Rejected: a pure `build.rs` cannot call
`registry()` (crate not yet compiled), so "emit on every build" degrades to a test-layer
golden-file dance with a `BLESS` escape hatch — strictly more moving parts than making
the JSON authoritative. The inversion removes the dump command, the generated-file
concept, and the byte-diff test entirely.

## Observable signal

`bash parish/testing/fixtures/mcp_cold_register.sh` prints, per scenario, the raw
`initialize` / `tools/list` / `tools/call` responses and PASS/FAIL lines mapped to the
acceptance criteria. `cargo test -p parish-mcp` covers the manifest-sourced registry +
the name↔translate bijection.

## Feature flag

Not gameplay; no runtime feature flag. The behavior is launcher/build wiring. The
generic-tauri-backend cargo feature is untouched.
