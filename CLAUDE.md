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
├── main.rs          # Entry point, tokio runtime init
├── lib.rs           # Module declarations
├── error.rs         # ParishError (thiserror)
├── tui/             # Ratatui terminal UI
├── world/           # World state, location graph, time system
├── npc/             # NPC data model, behavior, cognition tiers
├── inference/       # Ollama HTTP client, inference queue
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

- Architecture & subsystem design: `docs/design/`
- Architecture decisions: `docs/adr/`
- Roadmap & status: `docs/requirements/roadmap.md`
- Implementation plans: `docs/plans/`
- Original monolithic design: `DESIGN.md` (archival)
- Development journal: `docs/journal.md` (cross-session notes and recommendations)
