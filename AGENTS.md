# Repository Guidelines

## Project Structure & Module Organization
Parish is a Cargo workspace with shared engine code in `crates/parish-core/`, runtime entry points in `src/`, the Axum web backend in `crates/parish-server/`, and the Tauri desktop backend in `src-tauri/`. The Svelte 5 frontend lives in `ui/src/`. Integration tests sit in `tests/`, scripted gameplay fixtures in `tests/fixtures/`, game content in `mods/kilteevan-1820/`, and longer-form design notes in `docs/`.

Put reusable gameplay logic in `crates/parish-core`; keep transport-specific wiring in `src/`, `src-tauri/`, or `crates/parish-server/`.

## Build, Test, and Development Commands
Prefer `just` recipes over ad hoc commands:

- `just build` builds the Rust workspace.
- `just run`, `just run-tui`, `just run-headless` start the app in desktop, terminal UI, or plain REPL mode.
- `just ui-dev` runs the Svelte frontend alone; `just tauri-dev` starts the full desktop stack.
- `just test`, `just ui-test`, and `just ui-e2e` run Rust, Vitest, and Playwright suites.
- `just check` runs `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test`.
- `just verify` is the pre-push path: quality gates plus the walkthrough harness.

## Coding Style & Naming Conventions
Rust uses `cargo fmt` defaults: 4-space indentation, `snake_case` modules/functions, and `CamelCase` types. Add `///` doc comments to public APIs. Use `thiserror` in library code and `anyhow` in binaries.

Frontend code follows the existing Svelte/TypeScript style: tab-indented files, `PascalCase.svelte` components, and `camelCase` stores/utilities. Keep IPC types in `ui/src/lib/types.ts` aligned with Rust `serde` output, including `snake_case` field names where required.

## Testing Guidelines
Add tests with every behavior change. Rust integration tests belong in `tests/*_integration.rs`; UI tests are colocated as `*.test.ts`. Use `cargo test <name>` for focused Rust runs, `npx vitest run` in `ui/` for component tests, and `cargo run -- --script tests/fixtures/test_walkthrough.txt` for harness validation. The repo target is to keep coverage above 90%.

## Commit & Pull Request Guidelines
Recent history follows conventional prefixes such as `feat:`, `fix:`, `refactor:`, `docs:`, and `test:`. Keep commits scoped to one logical change and use imperative summaries; issue references like `resolve #135` are common when applicable.

PRs should explain the behavior change, link related issues, list commands run (`just check`, `just verify`, UI tests), and include screenshots or updated Playwright baselines for visible UI changes.
