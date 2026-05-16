# Proof Evidence — PR #974: substitute upstream_url in Tauri tile snapshot

Evidence type: gameplay transcript
Date: 2026-05-16
Branch: worktree-fix-tile-tauri-proxy

## Requirement

After PR #955 introduced the `/tiles/{id}/...` same-origin proxy on
`parish-server`, the Tauri webview (which has no `/tiles/` handler) began
returning 404s on every raster tile request, breaking the historic-map
overlay in the desktop build. `TileSourceSnapshot::list_from_map_config`
needed a runtime-aware switch so each entry point selects the right URL.

## Fix

`list_from_map_config` now takes a `has_tile_proxy: bool` argument:

- `parish-server` passes `true` and keeps the proxy `url`.
- `parish-tauri` passes `false`. When `upstream_url` is set, the snapshot
  substitutes it so MapLibre fetches the absolute URL directly. When
  `upstream_url` is empty (e.g. OSM), the original `url` is kept.

## Unit tests

Command:

```sh
cargo test -p parish-core --lib ipc::types::tests::tile_snapshot
```

Two regression tests added in
`parish/crates/parish-core/src/ipc/types.rs`:

- `tile_snapshot_proxy_mode_uses_url` — asserts proxy mode still emits
  `/tiles/historic/...`.
- `tile_snapshot_no_proxy_substitutes_upstream` — asserts no-proxy mode
  substitutes `https://mapseries-tilesets.s3.amazonaws.com/...` for the
  historic source AND keeps `https://tile.openstreetmap.org/` for OSM
  (no upstream_url to substitute).

Both tests pass on the branch.

## Server wiring

`parish-server` (`src/lib.rs`) calls
`TileSourceSnapshot::list_from_map_config(&map_cfg, true)` because it
hosts the proxy.

## Tauri wiring

`parish-tauri` (`src/lib.rs`) calls the same with `false`, since the
webview has no `/tiles/` handler.

Result: Tauri webview gets working tile URLs; server keeps the proxy
path. Regression from #955 closed.
