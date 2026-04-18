# parish-config

Configuration loading and typed config models for Parish.

## Purpose

`parish-config` centralizes engine/provider/feature-flag configuration so every
runtime (CLI, server, Tauri) resolves settings the same way.

## Key modules

- `engine` — engine-level settings (runtime, simulation, speed, persistence).
- `provider` — inference provider definitions and category overrides.
- `flags` — runtime feature-flag parsing and querying.

## Notes

- Designed for parity across all modes.
- Re-exports `SpeedConfig` from `parish-types` for downstream convenience.
