# Code Style & Dependencies

## Rust

- `cargo fmt` defaults: 4-space indent, `snake_case` modules/functions, `CamelCase` types.
- Doc comments (`///`) on all public structs and functions.
- Use `thiserror` for library errors, `anyhow` in main/binary code.
- Prefer `match` over `if let` for enum exhaustiveness.
- Keep modules focused — one responsibility per file.
- No `#[allow]` without a justifying comment.

## Frontend

- Tab-indented Svelte/TypeScript files.
- `PascalCase.svelte` components, `camelCase` stores/utilities.
- Keep IPC types in `parish/apps/ui/src/lib/types.ts` aligned with Rust `serde` output, including `snake_case` field names.

## Key dependencies

| Crate / Package | Purpose |
|---|---|
| tokio | Async runtime (`features = ["full"]`) |
| tauri 2 | Desktop GUI framework |
| @tauri-apps/api v2 | TypeScript IPC bindings |
| svelte 5 + sveltekit | Frontend framework (static adapter) |
| reqwest | HTTP client for Ollama / LLM APIs |
| serde + serde_json | JSON serialization for LLM structured output |
| rusqlite | SQLite persistence (`features = ["bundled"]`) |
| anyhow / thiserror | Error handling |
| tracing | Structured logging |
| chrono | Time |
| axum | Web server |
| vitest + @testing-library/svelte | Frontend component tests |
| @playwright/test | E2E browser tests |
