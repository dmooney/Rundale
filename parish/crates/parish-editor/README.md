# parish-editor

Backend for the **Parish Designer** — the GUI editor that lets game designers
browse mods, edit NPC and location data, validate cross-references, persist
changes deterministically, and inspect save files.

This is a backend-agnostic leaf crate: it has no `tauri` / `axum` / `tower`
dependency and never touches the live gameplay session. The editor always
operates on a **fresh in-memory copy loaded from disk** and writes back
atomically, so editing and re-saving an unchanged mod file produces an empty
`git diff`.

## Module map

- `format` — deterministic 4-space-indent JSON serialization with atomic
  temp-and-rename writes (`write_json_deterministic`).
- `types` — serializable DTOs for the `/editor` IPC surface
  (`EditorModSnapshot`, `ValidationReport`, `ValidationIssue`, …).
- `mod_io` — granular, file-by-file mod loading (`list_mods`,
  `load_mod_snapshot`) so one broken file doesn't hide the rest.
- `validate` — cross-reference validator (`validate_snapshot`): orphan
  locations, relationship targets, schedule ranges, duplicate ids.
- `persist` — validation-gated, atomic save (`save_mod`, `SaveResult`, and the
  per-doc `save_*` helpers).
- `save_inspect` — read-only save-file inspector (`list_saves`,
  `list_branches`, `list_snapshots`, `read_latest_snapshot`).
- `live_reload` — hot-reload the live world graph from disk after a save while
  preserving runtime progress (`reload_world_graph_preserving_runtime`).

## Re-export

`parish-core` re-exports this crate as `parish_core::editor`, so existing
consumers (`parish-tauri`, `parish-server`, `parish-core::ipc::editor`) keep
their import paths unchanged.

## Scoped commands

```sh
cargo test -p parish-editor
cargo doc  -p parish-editor --no-deps --open
```
