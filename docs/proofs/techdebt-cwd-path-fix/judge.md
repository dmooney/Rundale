Evidence type: transcript

## Summary

TD-005 ("CWD-relative path fix") is a pure technical-debt cleanup. Every
call to `load_engine_config(None)` in the codebase was replaced with a
startup-time config-path resolution. The `None` option was removed from the
function signature, forcing all callers to supply an explicit path.

## Steps taken

1. Read TODO.md, all 4 affected source files, and the reference
   `resolve_project_saves_dir` implementation in `parish-persistence`.
2. Changed `load_engine_config` to require `&Path`, removed the CWD-relative
   `Path::new("parish.toml")` fallback.
3. Added `resolve_config_path(start)` that walks up ancestors looking for
   `parish.toml` — the same marker-probe pattern used by `resolve_project_saves_dir`.
4. Updated all three entry-point crates (cli, server, tauri) to resolve
   the config path at startup via `--config` flag or `resolve_config_path`.
5. Removed `test_load_engine_config_none` (exercised deleted `None` code path).
6. Ran full test suite and clippy — all clean.

## Verdict

Verdict: sufficient
Technical debt: clear

The change is mechanical, all tests pass, clippy is clean, and no gameplay
behavior was altered (missing config still returns `EngineConfig::default`).
