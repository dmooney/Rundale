# Parish — Claude Code Guide

## Build & Test

- Build: `cargo build`
- Release build: `cargo build --release`
- Run: `cargo run`
- Test all: `cargo test`
- Test one: `cargo test <test_name>`
- Format: `cargo fmt --check` (apply: `cargo fmt`)
- Lint: `cargo clippy -- -D warnings`

Run `cargo fmt`, `cargo clippy`, and `cargo test` before committing.

## Verification Before Pushing

**Always manually verify changes work before pushing.** Running tests alone is not enough — use the `GameTestHarness` to actually exercise your changes:

- Run `cargo run -- --script tests/fixtures/test_walkthrough.txt` and inspect the JSON output
- Write a quick ad-hoc script file to test the specific feature you changed
- If you added or changed game mechanics, write a targeted test script and run it through `--script` mode
- Only push after you've both run the test suite **and** visually confirmed the harness output looks correct

## Engineering Standards

Every commit **must** satisfy all of the following:

1. **Documentation**: Update relevant documentation for every commit. New public APIs, changed behavior, and architectural decisions must be reflected in doc comments (`///`), `docs/`, or ADRs as appropriate.
2. **Tests required**: All new code must have accompanying unit tests. No new function, struct, or module lands without test coverage.
3. **Coverage threshold**: Maintain test coverage above **90%**. Use `cargo tarpaulin` (or equivalent) to verify. PRs that drop coverage below 90% must not be merged.
4. **All standards must pass**: `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test` must all succeed. No exceptions, no `#[allow]` without a justifying comment.

## Architecture

See [docs/design/overview.md](docs/design/overview.md) for full architecture. See [docs/index.md](docs/index.md) for all documentation.

```
src/
├── main.rs          # Entry point, CLI args (clap), mode routing
├── lib.rs           # Module declarations
├── error.rs         # ParishError (thiserror)
├── config.rs        # Provider configuration (TOML + env + CLI)
├── headless.rs      # Headless stdin/stdout REPL mode
├── testing.rs       # GameTestHarness for automated testing
├── tui/             # Ratatui terminal UI
├── gui/             # egui windowed GUI (--gui flag)
├── world/           # World state, location graph, time, movement, encounters
├── npc/             # NPC data model, behavior, cognition tiers
├── inference/       # LLM client (OpenAI-compatible), queue, Ollama bootstrap
├── persistence/     # SQLite save/load, WAL journal
└── input/           # Player input parsing, command detection
```

## Code Style

- Follow `cargo fmt` output exactly
- All `cargo clippy` warnings are errors (`-D warnings`)
- Doc comments (`///`) on all public structs and functions
- Use `thiserror` for library errors, `anyhow` in main/binary code
- Prefer `match` over `if let` for enum exhaustiveness
- Keep modules focused — one responsibility per file

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| tokio | Async runtime (features = "full") |
| ratatui + crossterm | Terminal UI with 24-bit true color |
| eframe + egui | Windowed GUI mode (immediate-mode, cross-platform) |
| reqwest | HTTP client for Ollama API (`localhost:11434`) |
| serde + serde_json | JSON serialization for LLM structured output |
| rusqlite | SQLite persistence (features = "bundled") |
| anyhow / thiserror | Error handling |
| tracing | Structured logging |
| chrono | Time representation |

## Gotchas

- **Tokio + blocking**: Never use `std::thread::sleep` in async code; use `tokio::time::sleep`
- **Rusqlite is sync**: Wrap DB calls in `tokio::task::spawn_blocking`
- **Ratatui panic safety**: Always restore terminal state on panic (install panic hook)
- **Ollama**: Must be running on `localhost:11434` for inference calls
- **Reqwest timeouts**: Set explicit timeouts on all HTTP requests
- **Serde defaults**: Use `#[serde(default)]` for optional fields in LLM response structs

## Git Workflow

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`
- One logical change per commit
- Run full test suite before pushing

## Documentation Map

Start at [docs/index.md](docs/index.md) for the full hub. Key paths:

- **Architecture & design**: `docs/design/overview.md` → subsystem docs
- **Architecture decisions**: `docs/adr/README.md` → individual ADRs
- **Status tracking**: `docs/requirements/roadmap.md` (per-item checkboxes)
- **Implementation plans**: `docs/plans/` (one per phase)
- **Testing harness**: `docs/design/testing.md` (GameTestHarness, script mode)
- **Dev journal**: `docs/journal.md` (cross-session notes)
- **Known issues**: `docs/known-issues.md`
- **Archival**: `DESIGN.md` (original monolithic design, superseded by `docs/design/`)
