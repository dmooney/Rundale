# Repository Guidelines — Rundale on the Parish Engine

This file is unified with `AGENTS.md`. Keep both in sync.

Start with the detailed agent docs in [docs/agent/README.md](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, and Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout and module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — Tokio, SQLite, Ollama, mode parity pitfalls
- [git-workflow.md](docs/agent/git-workflow.md) — commits, tests, and PR standards
- [skills.md](docs/agent/skills.md) — `/check`, `/verify`, `/prove`, `/play`, etc.

**Rundale** is the game (Irish living world, 1820). **Parish** is the engine (Rust workspace + frontends).

## Current project state (quick map)

- Rust workspace crates under `crates/`:
  - `parish-core` (shared game logic)
  - `parish-cli` (CLI/headless binary `parish`)
  - `parish-server` (Axum web backend)
  - `parish-tauri` (Tauri desktop backend)
  - `geo-tool` (OSM extraction CLI)
- Frontend: `apps/ui/` (Svelte 5 + TypeScript)
- Game content: `mods/rundale/`
- Test fixtures: `testing/fixtures/`
- Deploy artifacts: `deploy/`
- Documentation hub: `docs/index.md`

## Non-negotiable engineering rules

1. **Module ownership:** Shared logic belongs in `crates/parish-core/` only. Do not duplicate shared modules in `crates/parish-cli/src/`.
2. **Mode parity:** Tauri, headless CLI, and web server must share behavior.
3. **Tests with behavior changes:** Add/adjust tests for every behavior change.
4. **Gameplay proof:** For gameplay features, run `/prove <feature>` (unit tests alone are not sufficient).
5. **No unexplained `#[allow]`:** Only with explicit justification.
6. **Feature flags for new engine/gameplay features:** Gate with `config.flags.is_enabled("feature-name")`, default-on, and document in PR.

## Standard commands

```sh
just build         # cargo build (default member parish-cli)
just run           # cargo tauri dev
just run-headless
just check         # fmt + clippy + tests
just verify        # check + harness walkthrough

just ui-test       # frontend unit tests
just ui-e2e        # Playwright end-to-end tests
just screenshots   # regenerate docs/screenshots/*.png
```

## Commit and PR expectations

- Conventional commits: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.
- One logical change per commit.
- PRs should explain behavior changes, link issues, list commands run, and include screenshots / updated Playwright baselines for visible UI changes.
