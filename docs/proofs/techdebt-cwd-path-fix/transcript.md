# TD-005: CWD-relative path fix for `load_engine_config`

## What was changed

`load_engine_config(path: Option<&Path>)` silently fell back to
`Path::new("parish.toml")` when called with `None`, making the path relative
to the process's current working directory at call time — a violation of
AGENTS.md Rule 9 ("resolve runtime paths from explicit config, not the cwd").

## Changes applied

### 1. `parish/crates/parish-config/src/engine.rs`

- **Signature change**: `load_engine_config` now takes `&Path` instead of
  `Option<&Path>`. The implicit `Path::new("parish.toml")` fallback was
  removed.
- **New helper**: Added `resolve_config_path(start: &Path) -> PathBuf`
  that walks up to 5 ancestor directories of `start` looking for an existing
  `parish.toml` file. If none is found, it returns `start.join("parish.toml")`
  (which will produce defaults when passed to `load_engine_config`). Must be
  called once at startup with a deliberately resolved starting directory.
- **Test `test_load_engine_config_none` removed**: This test exercised the
  `None`-fallback code path by calling `set_current_dir`. The behavior no
  longer exists. Remaining tests `test_load_engine_config_missing_file` and
  `test_load_engine_config_from_file` updated to pass `&Path` directly.

### 2. `parish/crates/parish-cli/src/main.rs` (the `parish` package)

- Uses the existing `--config` CLI argument if provided.
- Otherwise calls `resolve_config_path` with the CWD (probed once at startup).
- Passes the resolved path to `load_engine_config`.

### 3. `parish/crates/parish-server/src/lib.rs`

- Calls `resolve_config_path(&data_dir)` where `data_dir` is already a
  startup-resolved absolute path (passed to `run_server`).
- Passes the resolved path to `load_engine_config`.

### 4. `parish/crates/parish-tauri/src/lib.rs`

- Calls `resolve_config_path(&data_dir)` where `data_dir` is already a
  startup-resolved absolute path.
- Passes the resolved path to `load_engine_config`.

### 5. `parish/crates/parish-config/TODO.md`

- Moved TD-005 from "Follow-up" to "Done".

## Verification

```sh
# All tests pass (1 pre-existing failure in parish-tauri unrelated to this change:
# discover_save_files_returns_ok_for_missing_saves_dir)
cargo test -p parish-config -p parish -p parish-server -p parish-tauri

# Clippy clean
cargo clippy --workspace --all-targets -- -D warnings
```

## Files changed

| File | Change |
|------|--------|
| `parish/crates/parish-config/src/engine.rs` | Signature change, new `resolve_config_path`, test updates |
| `parish/crates/parish-cli/src/main.rs` | Resolve config path at startup |
| `parish/crates/parish-server/src/lib.rs` | Resolve config path at startup |
| `parish/crates/parish-tauri/src/lib.rs` | Resolve config path at startup |
| `parish/crates/parish-config/TODO.md` | TD-005 moved to Done |
