# parish-engine — agent scope

In-process engine entry point. Primary binary target and library. One of three mode-parity entry points (alongside `parish-server` and `parish-tauri`) — supports headless REPL (`--headless`), script runner (`--script FILE`), and default Tauri-launch. Re-exports all of `parish-core` under its own namespace. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/).

## Scoped commands

```sh
cargo test -p parish-engine                    # unit + integration
cargo run  -p parish-engine                    # default (launches Tauri)
cargo run  -p parish-engine -- --headless      # stdin/stdout REPL
cargo run  -p parish-engine -- --script FILE   # script harness (JSON output, no LLM)
just check                                     # full fmt+clippy+tests (workspace)
just run-headless                              # headless via justfile shortcut
```

The crate ships both a library (`parish_engine`) and a binary (`parish-engine`). The binary is `src/main.rs` (clap parser + launch dispatch); everything else is under the library surface so embedders (tests, script harness) can import modules directly.

## Local gotchas

- **Cross-runtime orchestration belongs in `parish-core` (rule #12).** New game-loop / IPC handlers must live in `parish-core` parameterized over `EventEmitter`; this crate provides only the CLI adapter (`StdoutEmitter` in `emitter.rs`, `CliCommandHost` in `command_host.rs`). Copy-pasting orchestration from `parish-server` or `parish-tauri` is forbidden (#687, #696).
- **Re-exports are load-bearing.** `parish_engine::world`, `parish_engine::npc`, `parish_engine::config`, etc. re-export from `parish_core`. External consumers import through `parish_engine`. Never remove a re-export without auditing every downstream crate.
- **Testing harness is shared infrastructure.** `GameTestHarness` and `run_script_mode` in `src/testing.rs` are used by integration tests across the workspace — API or behavior changes break tests in `parish-core`, `parish-server`, and play-fixture crates.
- **Select semantic test fixtures by stable identity.** Never use `HashMap` iteration or `all_npcs().next()` to choose a fixture whose name, role, title, or state affects the assertion; name the fixture explicitly and fail if it is absent so fresh-process results cannot depend on randomized order.
- **Mode parity.** Every gameplay action available over Tauri IPC or WebSocket must also be reachable via the headless CLI. When adding a new command, ensure `src/headless.rs` dispatches it.
- **Script mode skips LLM init entirely.** `--script FILE` invokes `run_script_mode` in `src/testing.rs`, which uses `SimulatorClient` and never touches Ollama or any cloud provider.
- **`#[deprecated]` `find_data_dir()` violates rule #9.** Marked deprecated in `src/main.rs` — new code must resolve runtime paths from explicit config on `AppState`, not `std::env::current_dir()`. Exists only until all paths are migrated to `saves_dir` / `log_app_name` on `App`.
- **`real_loop.rs` and `shadow.rs` are harness-correctness infrastructure (#1159).** `real_loop.rs` routes harness input through the real `game_loop` path; `shadow.rs` canonicalizes event streams so the legacy router and real loop outputs can be compared semantically. Do not remove either module.

## Module map

`app.rs` state + lifecycle, `headless.rs` REPL runtime, `testing.rs` harness + script runner, `real_loop.rs` real-loop harness path (#1159), `shadow.rs` differential harness comparison (#1159), `config.rs` engine-specific config re-exports, `command_host.rs` `CliCommandHost` (`SystemCommandHost` impl), `emitter.rs` `StdoutEmitter` (`EventEmitter` impl), `debug.rs` `/debug` command handlers, `main.rs` + `lib.rs` startup wiring + re-exports.
