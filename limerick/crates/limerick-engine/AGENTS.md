# limerick-engine — agent scope

In-process engine entry point. Primary binary target and library. One of three mode-parity entry points (alongside `limerick-server` and `limerick-tauri`) — supports headless REPL (`--headless`), script runner (`--script FILE`), and default Tauri-launch. Re-exports all of `limerick-core` under its own namespace. See root [`AGENTS.md`](../../../AGENTS.md) and [`docs/agent/`](../../../docs/agent/).

## Scoped commands

```sh
cargo test -p limerick-engine                    # unit + integration
cargo run  -p limerick-engine                    # default (launches Tauri)
cargo run  -p limerick-engine -- --headless      # stdin/stdout REPL
cargo run  -p limerick-engine -- --script FILE   # script harness (JSON output, no LLM)
just check                                     # full fmt+clippy+tests (workspace)
just run-headless                              # headless via justfile shortcut
```

The crate ships both a library (`limerick_engine`) and a binary (`limerick-engine`). The binary is `src/main.rs` (clap parser + launch dispatch); everything else is under the library surface so embedders (tests, script harness) can import modules directly.

## Local gotchas

- **[Cross-runtime orchestration](../../../docs/agent/engineering-rules.md#cross-runtime-orchestration) belongs in `limerick-core`.** New game-loop / IPC handlers must live in `limerick-core` parameterized over `EventEmitter`; this crate provides only the CLI adapter (`StdoutEmitter` in `emitter.rs`, `CliCommandHost` in `command_host.rs`). Copy-pasting orchestration from `limerick-server` or `limerick-tauri` is forbidden (#687, #696).
- **Re-exports are load-bearing.** `limerick_engine::world`, `limerick_engine::npc`, `limerick_engine::config`, etc. re-export from `limerick_core`. External consumers import through `limerick_engine`. Never remove a re-export without auditing every downstream crate.
- **Testing harness is shared infrastructure.** `GameTestHarness` and `run_script_mode` in `src/testing.rs` are used by integration tests across the workspace — API or behavior changes break tests in `limerick-core`, `limerick-server`, and play-fixture crates.
- **Select semantic test fixtures by stable identity.** Never use `HashMap` iteration or `all_npcs().next()` to choose a fixture whose name, role, title, or state affects the assertion; name the fixture explicitly and fail if it is absent so fresh-process results cannot depend on randomized order.
- **Mode parity.** Every gameplay action available over Tauri IPC or WebSocket must also be reachable via the headless CLI. When adding a new command, ensure `src/headless.rs` dispatches it.
- **Script mode skips LLM init entirely.** `--script FILE` invokes `run_script_mode` in `src/testing.rs`, which uses `SimulatorClient` and never touches Ollama or any cloud provider.
- **`#[deprecated]` `find_data_dir()` violates [runtime paths](../../../docs/agent/persistence-and-session-rules.md#runtime-paths).** Marked deprecated in `src/main.rs` — new code must resolve runtime paths from explicit config on `AppState`, not `std::env::current_dir()`. Exists only until all paths are migrated to `saves_dir` / `log_app_name` on `App`.
- **`real_loop.rs` and `shadow.rs` are harness-correctness infrastructure (#1159).** `real_loop.rs` routes harness input through the real `game_loop` path; `shadow.rs` canonicalizes event streams so the legacy router and real loop outputs can be compared semantically. Do not remove either module.
- **Freeze the game clock in sequential mode-parity fixtures.** The default clock runs at 36x, so host load can advance nested gameplay timestamps between legacy and real-loop executions even after a snapshot restore. Construct parity fixtures through the paused-clock helper and keep a delay-based regression assertion.
- **Headless execution uses package/binary `limerick-engine` and library `limerick_engine`.** The HTTP client owns the separate `limerick` binary; release archives must keep these names distinct.
- **`--script` runs `run_script_mode` → `GameTestHarness`**, NOT `run_headless`. Different code path. Wire features in both.
- **`GameTestHarness::new()` runs in hundreds of tests.** Never default a side-effecting writer (filesystem, network) on in `new()` — `new_with_<feature>()` opt-in or env-var gate is the pattern. See `character_log` opt-in for example.
- **`limerick-flags.json` is not loaded by `GameTestHarness`.** Runtime `/flag enable/disable` doesn't persist across script runs in the harness. For test-time flag behaviour, set `app.flags` directly.
- **`limerick --script` uses the active mod from `mods/mod-list.toml`.** Live script mode follows `active_setting` (`mods/rundale`, the tiny world); pass `--game-mod` to play anything else.
- **A long `/wait` in `--script` moves the clock in one step, then pumps schedules once.** NPCs only set off for their next scheduled place when the wait ends, so follow it with a short `/wait` (or step `advance_time(1)`) before checking presence.
- **CLI flag is `--game-mod <DIR>`, not `--mod`.** `limerick --script ... --mod mods/rundale` errors with "unexpected argument". Use `--game-mod` (env: `LIMERICK_MOD`).
- **`GameTestHarness::execute_via_real_loop` can now drive dialogue.** It originally wired `inference_queue = None`, so the real-loop path short-circuited on "LLM not configured" and only shadow-tested look/system commands (#1159). It now spins a mock-backed `spawn_inference_worker` (serving `harness.mock()` completions) for the duration of the call, so `handle_npc_conversation` runs end-to-end. Script the reply with `harness.mock().push_for("<npc-name-substring>", "<reply>")` before calling it.
- **Use `/stub` fixtures for deterministic post-generation dialogue-guard proofs.** Script-mode canned replies run through `guard_canned_npc_dialogue` before the shared dialogue apply seam, so they can prove wording polish without depending on a live model reproducing the bad phrase.
- **Movement commands target locations, not NPC names, in `GameTestHarness` tests.** For deterministic dialogue fixtures, move to the NPC's scheduled location (e.g. `go to Connolly's Shop`) before `/stub` + `talk to ...`; `go to Roisin Connolly` can fail as `NotFound`.
- **Empty-location dialogue has a GameTestHarness shortcut.** Script-mode `hello there` at a location with no NPCs goes through `handle_npc_interaction`, not the shared live-loop idle-message path; update both `testing.rs` unit tests and `tests/game_harness_integration.rs` when changing that fallback.
- **Dialogue proof oracles differ by harness.** Real-loop scenarios receive the completed NPC text in `stream-token` events (the `text-log` is only a streaming placeholder); legacy `--script` proofs need `/stub Name: reply` plus `talk to Name about ...` when they claim that a dialogue turn continued.
- **Rundale's default start graph cannot expose Tier 4 runtime wiring.** Every authored NPC sits within four BFS edges of the start, while Tier 4 begins beyond five; parity/lifecycle tests must install a synthetic or authored distance-six NPC and assert both the recorded tick and a delivered semantic event (#1865).

## Module map

`app.rs` state + lifecycle, `headless.rs` REPL runtime, `testing.rs` harness + script runner, `real_loop.rs` real-loop harness path (#1159), `shadow.rs` differential harness comparison (#1159), `config.rs` engine-specific config re-exports, `command_host.rs` `CliCommandHost` (`SystemCommandHost` impl), `emitter.rs` `StdoutEmitter` (`EventEmitter` impl), `debug.rs` `/debug` command handlers, `main.rs` + `lib.rs` startup wiring + re-exports.
