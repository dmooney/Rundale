# parish-mcp

A [Model Context Protocol](https://modelcontextprotocol.io) server that lets
LLM clients (Claude Code, Claude Desktop, etc.) drive a running
Parish/Rundale instance — and, in the future, any Tauri app — over JSON-RPC.

## What it does today

Exposes a small, curated set of MCP tools that map onto Parish's IPC surface:

| Tool | Effect |
| --- | --- |
| `parish_world_snapshot` | Read the current world snapshot. |
| `parish_map` | Read the location graph plus the player's position. |
| `parish_npcs_here` | List NPCs co-located with the player. |
| `parish_save_state` | Read save-file / branch metadata. |
| `parish_submit_input` | Send player input (movement, action, dialogue). |
| `parish_new_game` | Start a fresh game on a new branch. |
| `parish_save_game` | Save the current branch. |
| `parish_load_branch` | Load a named branch by id. |
| `tauri_invoke` | Generic escape hatch — call any backend command by name. |

Behind the scenes these go through a [`TauriBackend`](src/backend.rs) trait. The
default implementation (`ParishHttpBackend`) talks to a running
`parish-server` over its `/api/*` HTTP routes; because those routes are
**mode-parity** with the Tauri IPC commands (verified by
`parish-core/tests/wiring_parity.rs`), driving the server is equivalent
to driving the desktop app for everything that participates in the
parity sensor.

A second impl, `GenericTauriBackend`, is a stub for a future
[WebDriver / `tauri-driver`](https://v2.tauri.app/develop/tests/webdriver/)
backend that would drive any Tauri app's webview directly. It is wired
through the same trait so the MCP layer needs no changes when it lands.

## Transport

The binary speaks **stdio JSON-RPC 2.0**, the standard MCP transport for
Claude Code and Claude Desktop. Each line of stdin is one JSON-RPC
message; each line of stdout is one response. Logs go to stderr.

If you need an HTTP/SSE transport later, the `serve()` function in
[`src/jsonrpc.rs`](src/jsonrpc.rs) is generic over `AsyncRead`/`AsyncWrite`
and can be reused unchanged — only the entry point in `main.rs` would
need a second wiring path.

## Running it

There are two backends parish-mcp can drive — pick whichever matches what you
want to control.

### A. Live desktop session (recommended)

Run the Tauri desktop app with the embedded MCP bridge enabled:

```sh
cargo run -p parish-tauri -- --mcp-port 3030
```

The desktop window comes up as usual; in parallel, an in-process Axum
listener binds `127.0.0.1:3030` and exposes the same `/api/*` routes as
`parish-server`, sharing the live `AppState` and `tauri::AppHandle`. Every
write the MCP client sends — `submit_input`, `new_game`, `save_game` —
mutates the running window's world and triggers the same UI events the user
would see if they typed in the input box.

Then run parish-mcp pointed at the same port:

```sh
cargo run -p parish-mcp -- --base-url http://127.0.0.1:3030
```

### B. Headless `parish-server` session

Useful when you want a separate, headless session (e.g. for batched
evaluation or CI). Start a server in one terminal:

```sh
cargo run -p parish --bin parish -- web --port 3030
```

Then run parish-mcp pointed at it:

```sh
cargo run -p parish-mcp -- --base-url http://127.0.0.1:3030
```

The MCP client sees an identical tool surface either way — that's what
`parish-core/tests/wiring_parity.rs` enforces.

It will block waiting for JSON-RPC messages on stdin. From another
shell you can poke it directly:

```sh
echo '{"jsonrpc":"2.0","method":"initialize","id":1}' | \
  cargo run -p parish-mcp --quiet
```

## Wiring it into Claude Code / Claude Desktop

Add an entry like this to your MCP client config (paths and flags
adjusted for your checkout):

```json
{
  "mcpServers": {
    "parish": {
      "command": "/path/to/Rundale/target/debug/parish-mcp",
      "args": ["--base-url", "http://127.0.0.1:3030"]
    }
  }
}
```

If the target server is gated behind Cloudflare Access, set
`PARISH_MCP_AUTH_EMAIL` (or pass `--auth-email`) and the value will be
forwarded as `Cf-Access-Authenticated-User-Email`.

## Testing

```sh
cargo test -p parish-mcp
```

The crate ships unit tests for:

- JSON-RPC framing (round-trip, notifications, parse errors)
- backend HTTP behaviour (against a `wiremock` mock server)
- tool argument validation and command translation
- MCP handshake and `tools/call` dispatch
