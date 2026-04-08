# Gotchas

- **Module ownership**: All shared game logic lives in `crates/parish-core/`. The `parish-cli` crate re-exports via `pub use parish_core::X`. **Never** create duplicate modules in `parish-cli/src/` — modify `parish-core` instead. The cli crate only contains binary-specific code: `main.rs`, `headless.rs`, `testing.rs`, `app.rs`, `config.rs`, `debug.rs`.
- **Tokio + blocking**: Never use `std::thread::sleep` in async code; use `tokio::time::sleep`.
- **Rusqlite is sync**: Wrap DB calls in `tokio::task::spawn_blocking`.
- **Ollama**: Must be running on `localhost:11434` for inference calls.
- **Reqwest timeouts**: Set explicit timeouts on all HTTP requests.
- **Serde defaults**: Use `#[serde(default)]` for optional fields in LLM response structs.
- **Mode parity**: All modes (Tauri, CLI/headless, web server, future modes) must have feature parity. Implement shared logic in `parish-core/` and wire it from every entry point.
- **Tauri IPC types**: `apps/ui/src/lib/types.ts` must match Rust serde output exactly (snake_case field names).
- **Test fixtures path**: Integration tests run with cwd = crate root, so they reference `../../testing/fixtures/...` and `../../mods/kilteevan-1820/...`.
