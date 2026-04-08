# Parish — Claude Code Guide

Detailed guides live in [docs/agent/](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout & module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — tokio, rusqlite, ollama, mode parity
- [git-workflow.md](docs/agent/git-workflow.md) — commits, standards, /prove
- [skills.md](docs/agent/skills.md) — `/check`, `/verify`, `/prove`, `/play`, ...

## Non-negotiable rules

1. **Module ownership.** Shared game logic lives only in `crates/parish-core/`. The `parish-cli` crate re-exports it. Never duplicate modules under `crates/parish-cli/src/`.
2. **Mode parity.** Tauri, headless CLI, and the web server must all share behavior — implement in `parish-core` and wire from every entry point.
3. **Tests on every change.** Coverage must stay above 90% (`cargo tarpaulin`). Run `/check` before any commit and `/verify` before any push.
4. **Prove gameplay features.** After implementing any gameplay change, run `/prove <feature>` — unit tests passing is not enough.
5. **No `#[allow]`** without a justifying comment.

## Quick start

```sh
just build       # cargo build (default member = parish-cli)
just run         # cargo tauri dev (desktop)
just run-headless
just check       # fmt + clippy + tests
just verify      # check + harness walkthrough
```

See [docs/index.md](docs/index.md) for the documentation hub.
