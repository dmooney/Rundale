# parish-cli (package: `parish`)

Headless/terminal entry point and runtime wiring for Parish.

## Purpose

This crate provides the primary binary (`parish`) and library helpers for
starting the game in CLI/headless workflows, with shared setup used in tests
and server-oriented execution paths.

## Key modules

- `main` — executable entry point.
- `app` — top-level startup and mode routing.
- `headless` — terminal REPL loop.
- `config` — runtime config loading/wrapping.
- `debug` / `testing` — diagnostics and test support helpers.

## Notes

Shared gameplay logic must live in `parish-core`; this crate should stay as an
entry-point/orchestration layer.
