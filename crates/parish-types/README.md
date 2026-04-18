# parish-types

Shared foundational types for the Parish engine.

## Purpose

`parish-types` is the leaf crate used by the rest of the workspace. It contains
stable, serialization-friendly types and helpers that should not depend on
higher-level gameplay modules.

## Key modules

- `ids` — strongly typed IDs and core world entity structs.
- `time` — game clock, seasons, festivals, and speed settings.
- `events` — event bus and game event definitions.
- `conversation` and `gossip` — shared narrative/social data structures.
- `error` — `ParishError` and cross-crate error variants.
- `dice` — deterministic and random utility rolling helpers.

## Used by

All `parish-*` engine crates (`parish-core`, `parish-world`, `parish-npc`,
`parish-server`, etc.). Keep this crate dependency-light and broadly reusable.
