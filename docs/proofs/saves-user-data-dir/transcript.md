Evidence type: gameplay transcript

# Saves + tile cache → platform user-data dir (PR #985)

## What

Moves saves and the map tile cache from project-relative `<repo>/saves/{,tile-cache/}` to a platform-native per-user data root named after the active mod's `app_name`:

- macOS: `~/Library/Application Support/Rundale/{saves,tile-cache}`
- Linux: `$XDG_DATA_HOME/rundale/{saves,tile-cache}`
- Windows: `%APPDATA%\Rundale\{saves,tile-cache}`

Engine-only fallback (no mod loaded) uses `"Parish"`.

## Why

`<repo>/saves/` was wrong for any packaged build, daemonised server, or `cargo run` from `/tmp`. The picker walked up the directory tree looking for `mods/rundale/world.json` to anchor saves — fragile, surprising, and broken outside dev. Rule #9 (#771) bans cwd-derived runtime paths.

Rundale is one mod of many for the Parish engine, so the user-data root name has to be mod-driven, not engine-driven — a second mod must not share a save folder with Rundale.

## Changes

- New `parish_persistence::paths::resolve_user_data_dir(app_name)`. Mirrors `parish_config::resolve_user_config_dir` but uses `XDG_DATA_HOME` on Linux.
- `picker::resolve_project_saves_dir` now takes `app_name: &str`; marker-walk + `_from_cwd` shim deleted.
- `ModMeta` gains optional `save_root: Option<String>` + `app_name()` helper (`save_root` → `name` fallback). Rundale `mod.toml` sets `save_root = "Rundale"` explicitly.
- All three entry points (parish-server, parish-tauri, parish-cli) pass `gm.manifest.meta.app_name()` (fallback `DEFAULT_APP_NAME = "Parish"`).
- `init_tile_cache` now anchored on the same user-data root, sibling of saves.
- **Bug fix:** CLI `/load` no longer hard-codes `PathBuf::from("saves")` relative to cwd; reads `app.saves_dir` resolved once at startup.
- Env overrides preserved + added: `PARISH_SAVES_DIR`, `PARISH_TILE_CACHE_DIR`, new `PARISH_USER_DATA_DIR` (root override; mostly for tests + ops).

## Commands run

```
cargo fmt --all --check                                # clean
cargo clippy --workspace --all-targets -- -D warnings  # clean
cargo test --workspace                                 # 2764 passed, 15 ignored (67 suites)
```

## Live smoke — parish-server (macOS)

Started `parish --web 3030` from the repo. The Rundale user-data dir was empty beforehand.

```
$ ls -la ~/Library/Application\ Support/Rundale/saves/
7467b5c1-1415-4932-8945-f532bbaed99d/
a45c0546-21da-44c9-a97d-3ce2937ad4a3/
b94c5cf1-ca3d-45bf-ba07-6e366b37370d/
sessions.db  20.0K

$ ls -la ~/Library/Application\ Support/Rundale/tile-cache/
(empty — no tile requests yet)
```

Per-session DBs landed at the new location; `sessions.db` (identity store) also lands there. `tile-cache/` was created at server startup as the sibling of `saves/`.

## Live smoke — parish CLI from /tmp

Confirms the marker-walk drop: running from `/tmp` no longer leaks a `./saves` folder beside the CLI invocation.

```
$ mkdir -p /tmp/parish-smoke-cli && cd /tmp/parish-smoke-cli && rm -rf saves
$ parish --provider simulator --script /tmp/parish-smoke.script
{"command":"/quit","result":"quit","location":"The Crossroads","time":"Morning","season":"Spring"}
$ ls /tmp/parish-smoke-cli/
logs
```

No `/tmp/parish-smoke-cli/saves/` was created. Pre-change, the marker walk would have anchored saves to the cwd.

## Live smoke — env-var override sanity

Both `PARISH_SAVES_DIR` and `PARISH_TILE_CACHE_DIR` continue to override the resolver. Verified by starting the server with both set to /tmp paths and checking the dirs were created exactly there:

```
$ PARISH_SAVES_DIR=/tmp/parish-override-saves \
  PARISH_TILE_CACHE_DIR=/tmp/parish-override-tiles \
  parish --web 3031 &
$ ls -la /tmp/parish-override-saves/
fc24a479-3031-40a5-a69b-514761530bd6/
sessions.db  20K
$ ls -la /tmp/parish-override-tiles/
(empty — no tile requests, but dir created)
```

Both env vars honoured.

## Test additions

- `parish-persistence/src/paths.rs` — new module with platform-data-dir tests gated by env mutex (env-override creates dir, empty env ignored, macOS HOME path under Application Support, Linux XDG_DATA_HOME + .local/share fallback).
- `parish-persistence/src/picker.rs` — rewrote env-override test to new `(app_name)` signature, added `test_resolve_project_saves_dir_uses_user_data_dir` and `test_resolve_project_saves_dir_empty_env_ignored`. Deleted obsolete marker-walk tests.
- `parish-core/src/game_mod.rs` — added `test_mod_meta_app_name_falls_back_to_name` and `test_mod_meta_app_name_uses_save_root_when_set`; extended `test_load_mod_from_directory` to assert additive schema round-trip (`save_root.is_none()` for older manifests).
