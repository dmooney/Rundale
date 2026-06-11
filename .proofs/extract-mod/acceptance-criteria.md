# Acceptance Criteria: extract-mod

## Task

Extract `parish/crates/parish-core/src/game_mod/` (~2,116 lines, the content-mod
loader) into a new workspace crate `parish-mod`, with `parish-core` re-exporting the
module as `parish_core::game_mod` so all ~48 consumer files keep compiling without
touching their import paths. A prerequisite sub-step must relocate
`ThemePalette` (currently in `parish-core/src/ipc/types.rs`, where it is an IPC type,
but referenced via `crate::ipc::ThemePalette` inside `game_mod/types.rs`) to
`parish-types`, with a backward-compat re-export shim at its current path
(`parish_core::ipc::ThemePalette`). The move must not change the serialized JSON
shape. This is a pure behaviour-preserving refactor.

## Criteria

1. **New crate `parish-mod` in workspace members** — observable via:
   `grep -w "parish-mod" parish/Cargo.toml` prints a `members` entry; `cargo
build -p parish-mod` succeeds without errors.

2. **`parish-core` re-export shim keeps `parish_core::game_mod::` paths
   compiling unchanged** — observable via: `cargo build -p parish-core` and
   `cargo build -p parish-server` and `cargo build -p parish-tauri` and
   `cargo build -p parish-engine` all succeed; none of the ~48 consumer files
   under `parish/crates/` have their `use parish_core::game_mod::` or
   `use crate::game_mod::` lines changed.

3. **`game_mod/` directory removed from `parish-core/src/`** — observable via:
   `ls parish/crates/parish-core/src/game_mod/` fails with "No such file or
   directory". No `game_mod` sub-module files remain in `parish-core/src/`.

4. **No logic duplication (rule #1)** — observable via: the `parish-mod` crate
   is the sole owner of the mod-loader logic; `parish-core/src/lib.rs` only
   holds a `pub use parish_mod as game_mod;` or `pub mod game_mod { pub use
parish_mod::*; }` shim (no copy of `GameMod`, `ModManifest`, `UiConfig`,
   etc. in `parish-core`).

5. **`ThemePalette` relocated to `parish-types`** — observable via:
   `grep -r "struct ThemePalette" parish/crates/parish-types/src/` prints the
   struct definition. `grep -r "struct ThemePalette"
parish/crates/parish-core/src/ipc/` prints nothing (struct gone from ipc).
   `use parish_core::ipc::ThemePalette;` still compiles — the shim re-export
   `pub use parish_types::ThemePalette;` is in `parish-core/src/ipc/types.rs`
   or `parish-core/src/ipc/mod.rs`.

6. **ThemePalette serde shape unchanged** — observable via: the existing
   `ThemePalette` unit test in `parish-core/src/ipc/types.rs` passes
   (hex-color round-trip); `types-manifest.json` fields for any type that
   includes `ThemePalette` remain unchanged (or the manifest is updated only
   for source location, not for field names or types). A
   `serde_json::to_value(&ThemePalette{…})` in a test must produce exactly
   `{"bg","fg","accent","panel_bg","input_bg","border","muted"}` —
   the same keys the TS interface `types.ts:88` declares.

7. **Architecture fitness tests updated but not silenced (rule #1)** —
   observable via: `cargo test -p parish-core --test architecture_fitness`
   green; the `BACKEND_AGNOSTIC` list in `architecture_fitness.rs` either
   includes `parish-mod` or the test is updated to check it; no `#[allow]`
   or test annotation mutes a real violation.

8. **`just check` green** — observable via: `just check` exits 0 (fmt + clippy
   - all workspace tests pass).

9. **Fixture output identical before/after** — observable via: running
   `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script
parish/testing/fixtures/play_extract-mod.txt` produces the same location
   names, mod metadata, and `/theme` data as the baseline recorded in the
   proof transcript; the Rundale mod name (`"Rundale"`) appears in `/status`
   output, and theme colour fields (`bg`, `fg`, `accent`) are non-empty
   hex strings.

10. **Rundale mod loads from a live entry point** — observable via: the startup
    log (or `/status` output) contains the mod name `"Rundale"` and a
    recognisable starting location (e.g. `"Ballyconnell"` or equivalent);
    `look` returns a non-empty location description containing period-flavour
    prose.

11. **Theme switching (`/theme`) still works** — observable via: the fixture
    command `/theme` (or `/debug theme` if the fixture syntax supports it)
    returns a JSON object whose keys include `bg`, `fg`, `accent`, `panel_bg`,
    `input_bg`, `border`, `muted`; the values are non-empty strings starting
    with `#`.

12. **`parish-mod` added to `BACKEND_AGNOSTIC` list (or equivalent guard)**
    — observable via: the architecture-fitness test either explicitly names
    `parish-mod` in the crate set it verifies for runtime-dep hygiene, or a
    new dedicated test covers it; adding `tauri` / `axum` to `parish-mod`'s
    `Cargo.toml` would cause `cargo test -p parish-core --test
architecture_fitness` to fail.

13. **Docs updated** — observable via:
    - `docs/agent/architecture.md` crate count updated from 16/17 to the new
      count and `parish-mod` appears in the workspace-crate table.
    - `docs/agent/codebase-map.md` (if it exists) updated to reference
      `parish-mod`.
    - README crate count / structure updated.
    - `just notices` run if `parish-mod` introduces new third-party deps
      (most likely none, since it only rearranges existing code).

## Verification script

Run:

```
cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_extract-mod.txt
```

Expected signals in output:

- A `status` or startup JSON line containing `"Rundale"` — confirms the mod
  loaded correctly from `parish-mod`.
- A `look` response with a non-empty `location_description` — confirms the
  world graph was loaded from the mod.
- A movement response (`go to …`) confirming travel between two named Rundale
  locations — confirms route resolution still works.
- A `/theme` response (if the engine script mode supports it) or equivalent
  that returns a JSON object with `bg`, `fg`, `accent` keys whose values are
  `#rrggbb` hex strings — confirms `ThemePalette` serialization is unchanged.
- `cargo test --workspace` exits 0 — confirms no compilation break in any
  consumer crate.
