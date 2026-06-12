# parish-editor — agent scope

Backend-agnostic leaf crate for the Parish Designer: mod browsing, per-file NPC/location/festival/encounter/anachronism loading and editing, cross-reference validation, deterministic atomic persistence, and read-only save-file inspection. Extracted from `parish-core` in #1409. Re-exported from `parish-core` as `parish_core::editor`; consumed only by `parish-core`. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-editor                     # unit tests (format, mod_io, persist, save_inspect, live_reload)
cargo test -p parish-editor -- --nocapture      # with stdout for debugging
```

## Gotchas

- **Leaf-crate dependency rule (rule #1).** Depends only on `parish-types`, `parish-world`, `parish-npc`, `parish-mod`, `parish-persistence`, `serde`, `serde_json`, `toml`. Never add `parish-core`, `tauri`, `axum`, or any runtime crate.
- **No live `GameMod` mutation.** The editor loads an independent `EditorModSnapshot` from disk and never touches the host application's active `GameMod` or `AppState`. Live world hot-reload after a save is the only path that touches `WorldState`, and it preserves runtime fields (clock, weather, visited nodes, edge traversals, player location).
- **Validation gates saves.** `save_mod` always re-runs `validate_snapshot` before writing. Error-severity issues block all writes; warnings do not. If even one file write fails after others have succeeded, already-committed writes are not rolled back.
- **Deterministic JSON is a hard invariant.** `write_json_deterministic` uses a 4-space indent and a trailing newline to match the on-disk `mods/rundale/*.json` convention. A round-trip through the editor must produce a byte-identical file (`save_mod_byte_identical_to_source` test enforces this). Never use `serde_json::to_string_pretty` or `HashMap` for map-typed fields in serialized structs — use `BTreeMap` to guarantee key order.
- **`EditorModSnapshot` wire format is IPC-stable.** `types.rs` defines the wire format for the `/editor` IPC commands; `EditorDoc`, `ValidationIssue`, `ValidationSeverity`, and `ValidationCategory` are serialized over the Tauri/Axum boundary. Renaming or reordering variants breaks the frontend.
- **`validate_snapshot` runs cross-reference checks via JSON round-trip.** `WorldGraph::load_from_str` is called on a freshly-serialized copy of the locations to get full parity with the game loader — do not replace this with a cheaper structural walk.

## Module map

`lib.rs` — crate root, re-exports, and module declarations. `types.rs` — IPC wire types: `EditorModSnapshot`, `ModSummary`, `EditorManifest`, `ValidationReport`, `ValidationIssue`, `ValidationSeverity`, `ValidationCategory`, `EditorDoc`. `mod_io.rs` — `list_mods` (mod browser scan) and `load_mod_snapshot` (granular per-file loading with parse-error accumulation). `validate.rs` — `validate_snapshot`: cross-reference checks for world graph, NPC location refs, relationships, schedules, associated NPCs, and ID uniqueness. `persist.rs` — `save_mod` and per-doc save functions (`save_npcs`, `save_world`, `save_festivals`, `save_encounters`, `save_anachronisms`); `SaveResult` enum. `format.rs` — `write_json_deterministic`: 4-space-indent pretty JSON + atomic temp-and-rename write. `live_reload.rs` — `reload_world_graph_preserving_runtime`: refreshes a live `WorldState` graph/locations from an updated `GameMod` while retaining runtime progress. `save_inspect.rs` — read-only save file browser: `list_saves`, `list_branches`, `list_snapshots`, `read_latest_snapshot`; returns raw `serde_json::Value` for schema-resilient snapshot display.
