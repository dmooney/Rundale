# limerick-core

Core gameplay orchestration for the Limerick engine.

## Purpose

`limerick-core` is the backend-agnostic engine crate. It composes world, NPC,
inference, input, and persistence crates into a coherent game session API used
by CLI, web server, and Tauri backends.

## Key modules

- `game_session` — runtime session state and orchestration.
- `dialogue_apply` — portable NPC dialogue grounding and the canonical
  validated apply boundary (re-exported from `game_session`).
- `portable_look` — portable `/look` rendering (re-exported from
  `ipc::commands::look`).
- `loading` / `game_mod` — mod and data loading.
- `ipc` — shared request/response/event types used by frontends.
- `editor` — Limerick Designer mod-editing support.
- `prompts` — prompt templates/assembly helpers.
- `debug_snapshot` — debug data structures for inspection tooling.

## Features

- `desktop` (default): the full desktop composition, including the
  local-inference bootstrap, the Designer backend, chronicle writers, and
  diagnostics.
- `mobile`: the portable configuration
  (`cargo check -p limerick-core --no-default-features --features mobile`).
  It excludes those desktop-only dependencies and the `ipc`, `game_loop`, and
  `game_session` modules that use them.

## Re-exports

Re-exports sub-crates (`config`, `inference`, `input`, `npc`, `persistence`,
`world`) to preserve stable import paths across entry-point crates.
