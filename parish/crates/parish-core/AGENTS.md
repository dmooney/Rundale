# parish-core — agent scope

Backend-agnostic composition + orchestration crate. Owns game session orchestration (`game_session`, `game_loop/`), the IPC layer (`ipc/`), and prompt templates. Re-exports all leaf crates (`chronicle`, `config`, `diagnostics`, `editor`, `inference`, `input`, `mod`, `npc`, `palette`, `persistence`, `providers`, `setup`, `types`, `world`) under stable paths. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-core                                # unit + arch fitness
cargo test -p parish-core --test architecture_fitness    # rule #1/#2 enforcement
cargo doc  -p parish-core --no-deps --open               # composed surface
```

## Local gotchas

- **Architecture-fitness test (rules #1, #2, #12).** Rejects backend-agnostic crates depending on `tauri`, `axum`, `tower*`, `wry`, `tao`, and orphan source files (on disk, no `mod` decl). Run before any commit adding `mod` declarations or crate deps.
- **`EventEmitter` is the seam (rules #12).** All new orchestration in `game_loop/`, IPC handlers, and payload structs must be defined here parameterized over `EventEmitter`; entry-point crates supply their own emitter (#687, #696). No copy-paste into `parish-server` / `parish-tauri` / `parish-engine`.
- **Re-exports are load-bearing.** Preserve `parish_core::*` names when refactoring — every entry point depends on them. Extraction shims (`crate::game_mod`, `crate::editor`, `crate::{character_log, location_log, chat_transcript}`, `crate::debug_snapshot`, `crate::ipc::bug_report`) are part of this contract.
- **Scaling guardrails (rule #11).** Edits to `AppState`, session persistence, real-time push, inference calls, identity lookups, or mod loading require review against [`docs/agent/scaling-rules.md`](../../../docs/agent/scaling-rules.md).

## Module map

`game_session.rs` runtime session, `game_loop/` tick + movement, `ipc/` shared types + handlers (incl. `bug_report` shim), `loading.rs`+`mod_source.rs` mod-load wiring (loader in `parish-mod`), `prompts/` templates, `event_bus.rs`, `identity.rs`, `secret_store.rs`, `session_store.rs`, `tile_cache.rs`. Extracted + re-exported: `parish-mod` (content-mod loader), `parish-editor` (Designer backend), `parish-chronicle` (character/location/chat logs), `parish-diagnostics` (debug snapshot + bug report).
