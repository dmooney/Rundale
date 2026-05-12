# TD-015: Remove CWD-relative fallback from `load_toml`

## What

`load_toml` in `parish/crates/parish-cli/src/config.rs` accepted `Option<&Path>` and fell back to `Path::new("parish.toml")` (a CWD-relative path) when `None` was passed. This violated AGENTS.md Rule 9: "runtime paths must be resolved from explicit config, not the cwd."

## Change

- Changed `load_toml` to require `&Path` (removed the `Option` wrapper and the CWD fallback)
- Inlined the now-trivial `load_toml` wrapper into its sole caller — `resolve_category_configs` now calls `read_toml_config` directly
- `resolve_category_configs` handles `None` config_path by using `TomlConfig::default()` directly instead of calling any file-resolution function

## Files changed

- `parish/crates/parish-cli/src/config.rs` — removed `load_toml` function, updated `resolve_category_configs` call site
- `parish/crates/parish-cli/TODO.md` — moved TD-015 from Open to Done

## Verification

```
cargo test -p parish       → 147 unit + 150 integration/eval/doc tests: all passed
cargo clippy -p parish --all-targets -- -D warnings  → clean
```
