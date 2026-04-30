# Gotchas

- **Module ownership**: All shared game logic lives in the leaf crates under `parish/crates/` (`parish-config`, `parish-inference`, `parish-input`, `parish-npc`, `parish-palette`, `parish-persistence`, `parish-world`, `parish-types`). `parish-core` composes them; `parish-cli` re-exports via `pub use parish_core::*`. **Never** create modules in `parish/crates/parish-cli/src/` that duplicate leaf-crate logic — extend the leaf crate instead. The cli crate only contains binary-specific code: `main.rs`, `headless.rs`, `testing.rs`, `app.rs`, `config.rs`, `debug.rs`. See [architecture.md](architecture.md) for the full crate map.
- **Tokio + blocking**: Never use `std::thread::sleep` in async code; use `tokio::time::sleep`.
- **Rusqlite is sync**: Wrap DB calls in `tokio::task::spawn_blocking`.
- **Ollama**: Must be running on `localhost:11434` for inference calls.
- **Reqwest timeouts**: Set explicit timeouts on all HTTP requests.
- **Serde defaults**: Use `#[serde(default)]` for optional fields in LLM response structs.
- **Mode parity**: All modes (Tauri, CLI/headless, web server, future modes) must have feature parity. Implement shared logic in a leaf crate and re-export through `parish-core`, then wire it from every entry point.
- **Tauri IPC types**: `parish/apps/ui/src/lib/types.ts` must match Rust serde output exactly (snake_case field names).
- **Test fixtures path**: Integration tests run with cwd = crate root, so they reference `../../parish/testing/fixtures/...` and `../../mods/rundale/...`.
