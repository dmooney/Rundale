# Acceptance Criteria: extract-editor

## Task

Extract the Parish Designer backend — `parish/crates/parish-core/src/editor/` (~1,673 lines
across 8 files: `format.rs`, `live_reload.rs`, `mod.rs`, `mod_io.rs`, `persist.rs`,
`save_inspect.rs`, `types.rs`, `validate.rs`) — into a new first-class workspace crate
`parish-editor`. `parish-core` keeps a thin shim re-exporting `parish_editor::*` as
`parish_core::editor`, so every existing consumer (`parish-tauri/src/editor_commands.rs`,
`parish-server/src/editor_routes.rs`, `parish-core/src/ipc/editor.rs`,
`parish-core/tests/normalize_mod.rs`, `parish-tauri/tests/input_validation.rs`) compiles
with zero import changes. This is a purely structural, behavior-preserving refactor.

## Criteria

1. **New crate exists and is registered in the workspace** — `parish/crates/parish-editor/`
   contains a `Cargo.toml` with `name = "parish-editor"` and `publish = false`; the entry
   appears in `parish/Cargo.toml` `[workspace] members` list; `parish/Cargo.toml`
   `[workspace.dependencies]` has a `parish-editor = { path = "crates/parish-editor" }`
   entry.
   Observable via: `cargo metadata --no-deps --manifest-path parish/Cargo.toml | jq
'.packages[].name' | grep parish-editor` exits 0 and prints `"parish-editor"`.

2. **`parish-core` depends on `parish-editor` and re-exports it as `parish_core::editor`** —
   `parish/crates/parish-core/Cargo.toml` has `parish-editor = { workspace = true }` in
   `[dependencies]`; `parish/crates/parish-core/src/lib.rs` has `pub mod editor;` pointing
   to a shim that does `pub use parish_editor::*;` (or equivalent `pub extern crate`).
   Observable via: `cargo doc -p parish-core --no-deps 2>&1 | grep editor` shows the
   `editor` module; `grep 'parish_core::editor' parish/crates/parish-server/src/editor_routes.rs`
   still resolves.

3. **No editor source files remain under `parish-core/src/editor/`** — the directory is
   removed entirely (or reduced to a single shim `mod.rs` if needed for re-export plumbing).
   No duplication of editor logic anywhere in the workspace.
   Observable via: `ls parish/crates/parish-core/src/editor/` either does not exist or
   contains only the shim file; `find parish/crates -path '*/parish-core/src/editor/format.rs' -o -path '*/parish-core/src/editor/validate.rs'` returns empty.

4. **`just check` passes without any new `#[allow]` suppressions** — `cargo fmt --check`,
   `cargo clippy`, and `cargo test` all pass workspace-wide. Architecture-fitness tests in
   `parish/crates/parish-core/tests/architecture_fitness.rs` pass, updated if they
   enumerate crates or modules (the `BACKEND_AGNOSTIC` list and the `no_orphaned_source_files`
   test in particular). If `parish-editor` should also be backend-agnostic (no
   `tauri`/`axum`/`tower*` deps), it must be added to `BACKEND_AGNOSTIC`; if deliberately
   excluded, a comment must explain why.
   Observable via: `just check` exits 0 with no errors.

5. **All existing consumer imports compile unchanged** — no `parish_core::editor::` path in
   any consumer file is modified. This covers:
   - `parish/crates/parish-server/src/editor_routes.rs` (imports `parish_core::editor::…`)
   - `parish/crates/parish-tauri/src/editor_commands.rs` (imports `parish_core::editor::…`)
   - `parish/crates/parish-core/src/ipc/editor.rs` (uses `crate::editor::…`)
   - `parish/crates/parish-core/tests/normalize_mod.rs` (uses `parish_core::editor::…`)
   - `parish/crates/parish-tauri/tests/input_validation.rs` (uses `parish_core::editor::…`)
     Observable via: `cargo test -p parish-core -p parish-server -p parish-tauri` passes;
     `cargo build -p parish-server -p parish-tauri` exits 0 with no import-related errors.

6. **Circular-dependency constraint satisfied** — `parish-editor` depends on `parish-core`
   types indirectly only through leaf crates. The three `crate::game_mod::*` imports
   in the editor source (`AnachronismData`, `EncounterTable`, `FestivalDef`, `ModManifest`,
   `GameMod`, `world_state_from_mod`) and the `crate::world::WorldState` import in
   `live_reload.rs` must not create a `parish-editor → parish-core → parish-editor` cycle.
   The implementor must choose one of:
   (a) move the required `game_mod` types to an appropriate leaf crate
   (`parish-config` or a new thin crate), or
   (b) keep `live_reload.rs` in `parish-core` (it depends on `GameMod` + `WorldState`
   which are defined there) and only extract the pure-I/O modules
   (`format`, `mod_io`, `persist`, `save_inspect`, `types`, `validate`) into
   `parish-editor`, with `live_reload` remaining in `parish-core::editor` and
   re-exported from the shim.
   Observable via: `cargo build -p parish-editor` exits 0; `cargo metadata --manifest-path
parish/Cargo.toml | jq '.resolve.nodes[] | select(.id | startswith("parish-editor"))
| .deps[].name' | grep 'parish-core'` returns empty (no dep cycle).

7. **Existing editor unit tests pass in the new crate** — tests from
   `persist.rs` (round-trip, idempotency, byte-identical, blocks-on-errors),
   `validate.rs` (rundale validates clean), `mod_io.rs` (list_mods, load_mod_snapshot),
   `format.rs` (atomic write, idempotent, trailing newline, round-trip),
   `live_reload.rs` (reload preserves runtime, prunes removed locations) all pass
   under the new crate.
   Observable via: `cargo test -p parish-editor` exits 0.

8. **Editor IPC commands still function end-to-end** — the editor protocol surface
   (`editor_list_mods`, `editor_open_mod`, `editor_get_snapshot`, `editor_validate`,
   `editor_update_npcs`, `editor_update_locations`, `editor_save`, `editor_reload`,
   `editor_close`, `editor_list_saves`, `editor_list_branches`, `editor_list_snapshots`,
   `editor_read_snapshot`) compiles and routes correctly in at least one live entry point.
   Observable via: either (a) `tauri_invoke` or HTTP `GET /api/editor-list-mods`
   returns a JSON array (not an error); or (b) the integration test
   `normalize_mod.rs::normalize_mod_source_integration` (run with
   `PARISH_NORMALIZE_MOD_DIR=mods/rundale`) completes without panic.

9. **Docs updated** — `docs/agent/architecture.md` crate table updated to 18 members,
   `parish-editor` row added; `docs/agent/codebase-map.md` `Parish Crates` table updated
   (count and new row); README crate count/structure section updated if it names the
   crate count. `just notices` run if `parish-editor` adds external Cargo deps.
   Observable via: `grep -n 'parish-editor' docs/agent/architecture.md docs/agent/codebase-map.md`
   matches in both files; `grep '17 crates\|17 member\|17 workspace' docs/agent/architecture.md`
   returns empty (old count gone).

10. **Behavior parity: the verification fixture produces identical engine output** — the
    game-play behavior (movement, look, NPC listing, status) is unaffected by the
    structural change.
    Observable via: `cargo run --manifest-path parish/Cargo.toml -p parish-engine --
--headless --script parish/testing/fixtures/play_extract-editor.txt` produces the same
    JSON output as the pre-refactor baseline; each line contains `"result":"looked"`,
    `"result":"system_command"` etc. matching the command type and no `"error"` or panic
    lines appear.

11. **Live proof** — the evidence bundle includes a live gameplay log using one of the
    accepted signals (headless `--script`, `just run-headless`, or `mcp__parish__*`) with
    each criterion mapped to the specific output line(s) that confirm it. The
    `evidence.md` header must declare `Evidence type: live gameplay transcript`.

## Coupling surprises (noted for implementor)

- **`live_reload.rs` depends on `crate::game_mod::{GameMod, world_state_from_mod}` and
  `crate::world::WorldState`** — both are defined in `parish-core` (not leaf crates),
  so naively moving `live_reload.rs` into a new `parish-editor` crate would create a
  `parish-editor → parish-core → parish-editor` circular dependency. See criterion 6
  for the two acceptable resolutions.

- **`types.rs`, `mod_io.rs`, `persist.rs` import
  `crate::game_mod::{AnachronismData, EncounterTable, FestivalDef, ModManifest}`** —
  these types live in `parish-core/src/game_mod/` (manifest.rs, types.rs). Extracting
  the editor requires either moving these types to a leaf crate (e.g. `parish-config`) or
  keeping a thin bridge in `parish-core` that feeds them as type parameters.

- **`ipc/editor.rs` uses `crate::editor::*` internally** — after extraction, this module
  would import `parish_editor::*` (or via the re-export shim `crate::editor::*`). No
  consumer import paths change, but `ipc/editor.rs` itself must be updated to use the
  shim or a direct `parish_editor::` import.

- **`parish-core`'s `architecture_fitness.rs` `BACKEND_AGNOSTIC` list and
  `no_orphaned_source_files` test** — both will need updating: the
  `no_orphaned_source_files` test will reject any editor `.rs` files that remain on
  disk without a `mod` declaration; the `BACKEND_AGNOSTIC` list needs a decision about
  whether `parish-editor` joins it (it should, since it has no runtime deps today).
  Also, `parish_engine_does_not_duplicate_parish_core_modules` compares top-level
  module names — if `editor` disappears from `parish-core/src/`, the shim is the only
  thing needed and the test should continue to pass without changes, but verify.

## Verification script

Run:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- \
  --headless --script parish/testing/fixtures/play_extract-editor.txt
```

Expected signals in output:

- `"command":"look","result":"looked"` — world description loads; mod content intact
- `"command":"/status","result":"system_command"` — game state accessible
- `"command":"/map","result":"system_command"` — world graph (mod locations) intact
- `"command":"/help","result":"system_command"` — command surface unchanged
- `"command":"/npcs","result":"system_command"` — NPC list accessible
- Exit code 0 — no `"error"` lines, no panics
