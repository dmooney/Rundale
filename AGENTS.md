# Repository Guidelines — Rundale on the Parish Engine

`AGENTS.md` is the source of truth for repo guidelines. `CLAUDE.md` is a symlink to it, so any edit here is automatically visible to Claude Code as well.

Start with the detailed agent docs in [docs/agent/README.md](docs/agent/README.md):

- [build-test.md](docs/agent/build-test.md) — cargo, harness, frontend, web, and Tauri commands
- [architecture.md](docs/agent/architecture.md) — workspace layout and module ownership
- [code-style.md](docs/agent/code-style.md) — Rust + Svelte conventions
- [gotchas.md](docs/agent/gotchas.md) — Tokio, SQLite, Ollama, mode parity pitfalls
- [git-workflow.md](docs/agent/git-workflow.md) — commits, tests, and PR standards
- [skills.md](docs/agent/skills.md) — `/check`, `/verify`, `/prove`, `/play`, etc.
- [harness.md](docs/agent/harness.md) — one-page map of every sensor, skill, and gate (start here when something fails)

**Rundale** is the game (Irish living world, 1820). **Parish** is the engine (Rust workspace + frontends).

## Current project state (quick map)

- Rust workspace: **all crates** under `crates/` — see [docs/agent/architecture.md](docs/agent/architecture.md) for the full table.
  - Binaries: `parish-cli` (CLI/headless `parish`), `parish-server` (Axum web), `parish-tauri` (desktop), `parish-geo-tool`, `parish-npc-tool`.
  - Composition: `parish-core` re-exports the leaf crates under stable namespaces.
  - Leaf logic crates: `parish-config`, `parish-inference`, `parish-input`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, `parish-types`.
  - These crates make up the **Parish** game engine.
- Frontend: `apps/ui/` (Svelte 5 + TypeScript)
- Rundale game content: `mods/rundale/`
- Test fixtures: `testing/fixtures/`
- Deploy artifacts: `deploy/`
- Documentation hub: `docs/index.md`

## Non-negotiable engineering rules

Rules marked **(enforced)** are checked mechanically by `cargo test` / CI — see `crates/parish-core/tests/architecture_fitness.rs`. The rest are still convention.

1. **Module ownership (enforced):** Shared logic belongs in a leaf crate (`parish-config`, `parish-inference`, `parish-input`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, `parish-types`). `parish-core` composes them. Do not duplicate leaf-crate logic in `crates/parish-cli/src/`. Orphaned source files (present on disk but not declared as `mod`) are also rejected.
2. **Mode parity (partially enforced):** Tauri, headless CLI, and web server must share behavior. The architecture-fitness test forbids backend-agnostic crates from depending on `tauri` / `axum` / `tower*` / `wry` / `tao`, so runtime-specific code can't leak into shared logic. Wiring parity (every IPC handler called from every entry point) is still convention.
3. **Tests with behavior changes:** Add/adjust tests for every behavior change.
4. **Gameplay proof:** For gameplay features, run `/prove <feature>` (unit tests alone are not sufficient).
5. **No unexplained `#[allow]`:** Only with explicit justification.
6. **Feature flags for new engine/gameplay features:** Gate with `config.flags.is_enabled("feature-name")`, default-on, and document in PR.
7. **Keep README.md up to date.** Especially the feature list, repository structure and credits. Run `just notices` to update third party notices when dependencies are changed.

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
