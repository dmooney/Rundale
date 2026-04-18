# parish-core

Core gameplay orchestration for the Parish engine.

## Purpose

`parish-core` is the backend-agnostic engine crate. It composes world, NPC,
inference, input, and persistence crates into a coherent game session API used
by CLI, web server, and Tauri backends.

## Key modules

- `game_session` — runtime session state and orchestration.
- `loading` / `game_mod` — mod and data loading.
- `ipc` — shared request/response/event types used by frontends.
- `editor` — Parish Designer mod-editing support.
- `prompts` — prompt templates/assembly helpers.
- `debug_snapshot` — debug data structures for inspection tooling.

## Re-exports

Re-exports sub-crates (`config`, `inference`, `input`, `npc`, `persistence`,
`world`) to preserve stable import paths across entry-point crates.
