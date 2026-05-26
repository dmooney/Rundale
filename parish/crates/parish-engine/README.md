# parish-engine (package: `parish-engine`)

Headless/terminal entry point and runtime wiring for Parish.

## Purpose

This crate provides the primary binary (`parish-engine`) and library helpers for
starting the game in CLI/headless workflows, with shared setup used in tests
and server-oriented execution paths.

## Key modules

- `main` — executable entry point and CLI argument parsing.
- `app` — shared `App` state (world, NPCs, config, inference).
- `headless` — terminal REPL loop and input handling.
- `command_host` — `CliCommandHost` (SystemCommandHost adapter for the CLI).
- `emitter` — `StdoutEmitter` (EventEmitter adapter for the CLI).
- `config` — engine-specific per-category config re-exports.
- `debug` — `/debug` command handlers for the headless CLI.
- `testing` — `GameTestHarness` and script-mode runner.

## Notes

Shared gameplay logic must live in `parish-core`; this crate should stay as an
entry-point/orchestration layer.
