# parish-core — agent scope

Backend-agnostic composition crate. Owns game session orchestration, IPC types, mod loading, prompts. Composed of leaf crates (`config`, `inference`, `input`, `npc`, `persistence`, `world`) which it re-exports under stable paths. See root [`AGENTS.md`](../../../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo test -p parish-core                                # unit + arch fitness
cargo test -p parish-core --test architecture_fitness    # rule #1/#2 enforcement
cargo doc  -p parish-core --no-deps --open               # composed surface
```

## Local gotchas

- **Architecture-fitness test enforces rules #1, #2, #12.** Backend-agnostic crates may not depend on `tauri`, `axum`, `tower*`, `wry`, `tao`. Orphan source files (on disk, no `mod` decl) are rejected. Run before any commit that adds `mod` declarations or new crate deps.
- **`EventEmitter` trait is the seam.** Any new orchestration (game loop, IPC handler, payload struct) must be defined here parameterized over `EventEmitter`; entry-point crates wire their own emitter (#687, #696). No copy-paste into `parish-server` / `parish-tauri` / `parish-engine`.
- **Re-exports are load-bearing.** Sub-crates are re-exported (`parish_core::config`, `parish_core::npc`, ...) — preserve names when refactoring or you break every entry point.
- **Scaling guardrails (rule #11).** Edits to `AppState`, session persistence, real-time push, inference calls, identity lookups, or mod loading require checklist review against [`docs/agent/scaling-rules.md`](../../../docs/agent/scaling-rules.md).

## Module map

`game_session/` runtime, `loading/`+`game_mod/` mod loading, `ipc/` shared types, `editor/` Designer support, `prompts/` templates, `debug_snapshot/` introspection.
