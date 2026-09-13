# limerick-types

Shared foundational types for the Limerick engine.

## Purpose

`limerick-types` is the leaf crate used by the rest of the workspace. It contains
stable, serialization-friendly types and helpers that should not depend on
higher-level gameplay modules.

## Key modules

- `ids` — strongly typed IDs and core world entity structs.
- `time` — game clock, seasons, festivals, and speed settings.
- `events` — event bus and game event definitions.
- `conversation` and `gossip` — shared narrative/social data structures.
- `error` — `LimerickError` and cross-crate error variants.
- `dice` — deterministic and random utility rolling helpers.
- `lib.rs` — root re-exports and shared types such as `AnachronismEntry`.

## Used by

All `limerick-*` engine crates (`limerick-core`, `limerick-world`, `limerick-npc`,
`limerick-server`, etc.). Keep this crate dependency-light and broadly reusable.
