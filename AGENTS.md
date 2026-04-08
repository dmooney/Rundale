# Repository Guidelines

Full agent docs live in [docs/agent/](docs/agent/README.md). Start there for build/test commands, workspace layout, code style, gotchas, and the git workflow.

## At a glance

- **Workspace**: all Rust crates under `crates/` (`parish-core`, `parish-cli`, `parish-server`, `parish-tauri`, `geo-tool`); Svelte frontend in `apps/ui/`; test fixtures in `testing/fixtures/`; game content in `mods/kilteevan-1820/`; deploy artifacts in `deploy/`.
- **Shared logic** belongs in `crates/parish-core/`. Transport-specific code (CLI, web server, Tauri) only orchestrates.
- **Build / test**: `just build`, `just check`, `just verify`. Frontend: `just ui-test`, `just ui-e2e`.
- **Commits**: conventional prefixes (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`), one logical change each.
- **Tests required** with every behavior change; coverage target ≥ 90%.

PRs should explain the change, link issues, list commands run, and include screenshots / updated Playwright baselines for visible UI changes.
