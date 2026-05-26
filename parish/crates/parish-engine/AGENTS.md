# parish-engine — agent scope

In-process engine entry point. Primary binary target and library. One of three mode-parity entry points (alongside `parish-server` and `parish-tauri`) — supports headless REPL (`--headless`), script runner (`--script FILE`), and default Tauri-launch. Re-exports all of `parish-core` under its own namespace for consumer convenience. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/) for repo-wide rules.

## Scoped commands

```sh
cargo test -p parish-engine                    # unit + integration
cargo run  -p parish-engine                    # default (launches Tauri)
cargo run  -p parish-engine -- --headless      # stdin/stdout REPL
cargo run  -p parish-engine -- --script FILE   # script harness (JSON output, no LLM)
just check                                     # full fmt+clippy+tests (workspace)
just run-headless                              # headless via justfile shortcut
```

The crate ships **both** a library (`parish_engine`) and a binary (`parish-engine`). The binary is `src/main.rs` (clap parser + launch dispatch); everything else lives under the library surface so embedders (tests, the script harness, future embedders) can keep using the module surface directly.

## Local gotchas

- **Cross-runtime orchestration belongs in `parish-core`** (rule #12). New game-loop / IPC handlers must live in `parish-core` parameterized over `EventEmitter`; this crate provides only the CLI adapter (`StdoutEmitter` in `emitter.rs`, `CliCommandHost` in `command_host.rs`). Copy-pasting orchestration from `parish-server` or `parish-tauri` is forbidden (#687, #696).
- **Re-exports are load-bearing.** `parish_engine::world`, `parish_engine::npc`, `parish_engine::config`, etc. all re-export from `parish_core`. External consumers (tests, tool crates) import through `parish_engine`. Never remove a re-export without auditing every downstream crate.
- **Testing harness is shared infrastructure.** `GameTestHarness` and `run_script_mode` in `src/testing.rs` are used by integration tests across the workspace — changes to their API or behaviour break tests in `parish-core`, `parish-server`, and play-fixture crates.
- **Mode parity.** Every gameplay action available over Tauri IPC or WebSocket must also be reachable via the headless CLI. When adding a new command or system action to `parish-core::ipc`, ensure the headless REPL loop (`src/headless.rs`) dispatches it.
- **Script mode skips LLM init entirely.** `--script FILE` invokes `run_script_mode` in `src/testing.rs`, which uses the `SimulatorClient` and never touches Ollama or any cloud provider — the script fixture provides canned responses.
- **`#[deprecated]` `find_data_dir()` violates rule #9.** This cwd-relative path resolution in `src/main.rs` is marked `#[deprecated]` — new code must resolve runtime paths from explicit config stored on `AppState`, not `std::env::current_dir()`. The deprecated function exists only until every code path has been migrated to the `saves_dir` / `log_app_name` fields on `App`.

## Module map

`app.rs` state + lifecycle, `headless.rs` REPL runtime (stdin→out, 2400+ lines), `testing.rs` harness + script runner (2700+ lines), `config.rs` engine-specific per-category config re-exports, `command_host.rs` `CliCommandHost` — `SystemCommandHost` impl for the CLI, `emitter.rs` `StdoutEmitter` — `EventEmitter` impl for the CLI, `debug.rs` `/debug` command handlers, `main.rs` + `lib.rs` startup wiring + re-exports.
