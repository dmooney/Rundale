Evidence type: code diff, test output, clippy output

Verdict: sufficient

Technical debt: clear

The CWD-relative fallback in `load_toml` was a clear violation of AGENTS.md Rule 9. The fix is minimal — removed the `Option` parameter, inlined the now-trivial wrapper, and pushed the default-config construction to the caller. All 297 tests pass and clippy is clean. No behavioural change for the CLI (the caller already received `config_path` from the `--config` flag as `Option<&Path>`, and when `None` we produce the same `TomlConfig::default()` that the old fallback produced when no `parish.toml` existed in the CWD).
