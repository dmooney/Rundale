# parish-core — agent scope

Backend-agnostic composition + orchestration crate. Owns game session orchestration (`game_session`, `game_loop/`), the IPC layer (`ipc/`), and prompt templates. Composes the leaf crates (`chronicle`, `config`, `diagnostics`, `editor`, `inference`, `input`, `mod`, `npc`, `palette`, `persistence`, `providers`, `setup`, `types`, `world`) and re-exports them under stable paths — the mod loader, Designer backend, chronicle writers, and debug-snapshot/bug-report subsystems live in their own crates now and are only re-exported here. See root [`AGENTS.md`](../../../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo test -p parish-core                                # unit + arch fitness
cargo test -p parish-core --test architecture_fitness    # rule #1/#2 enforcement
cargo doc  -p parish-core --no-deps --open               # composed surface
```

## Local gotchas

- **Architecture-fitness test enforces rules #1, #2, #12.** Backend-agnostic crates may not depend on `tauri`, `axum`, `tower*`, `wry`, `tao`. Orphan source files (on disk, no `mod` decl) are rejected. Run before any commit that adds `mod` declarations or new crate deps.
- **`EventEmitter` trait is the seam.** Any new orchestration (`game_loop/`, IPC handler, payload struct) must be defined here parameterized over `EventEmitter`; entry-point crates wire their own emitter (#687, #696). No copy-paste into `parish-server` / `parish-tauri` / `parish-engine`.
- **Re-exports are load-bearing.** Sub-crates are re-exported (`parish_core::config`, `parish_core::npc`, ...) — preserve names when refactoring or you break every entry point. The extraction shims (`crate::game_mod`, `crate::editor`, `crate::{character_log, location_log, chat_transcript}`, `crate::debug_snapshot`, `crate::ipc::bug_report`) are part of that contract.
- **Scaling guardrails (rule #11).** Edits to `AppState`, session persistence, real-time push, inference calls, identity lookups, or mod loading require checklist review against [`docs/agent/scaling-rules.md`](../../../docs/agent/scaling-rules.md).

## Module map

`game_session.rs` runtime session, `game_loop/` game tick + movement loop, `ipc/` shared types + handlers (incl. the `bug_report` re-export shim), `loading.rs`+`mod_source.rs` mod-load wiring (loader itself is `parish-mod`), `prompts/` templates, `event_bus.rs`, `identity.rs`, `secret_store.rs`, `session_store.rs`, `tile_cache.rs`. Extracted to sibling crates and re-exported: `parish-mod` (content-mod loader), `parish-editor` (Designer backend), `parish-chronicle` (character/location/chat logs), `parish-diagnostics` (debug snapshot + bug report).
