# Acceptance Criteria: generic-testbed-mod

## Task

Create a `mods/testbed/` setting mod that can be used in place of Rundale for pure engine
testing. The mod has a blueprint aesthetic (dark navy/cyan palette, monospace font, graph-paper
grid overlay on the map), a minimal five-location cross-shaped world, three NPC test agents,
and pig Latin as the code-switch language. A `mods/mod-list.toml` file selects which setting
mod is active, so both Rundale and Testbed can coexist in the `mods/` directory without
directory renaming. The engine's `discover_mods_in()` respects this file.

## Criteria

- **Mod loads** — the engine starts with `mods/mod-list.toml` pointing to `"testbed"` and
  no startup error; `world_snapshot` shows the correct mod state — observable via: CLI
  startup + `/status` showing `"Origin"` as the player's location.

- **Five locations navigable** — the world has exactly five locations (Origin, North Station,
  East Station, South Station, West Station) connected in a cross pattern; the player can
  traverse each edge — observable via: `go north`, `go east`, `go south`, `go west` all
  succeed from Origin and return to it.

- **NPCs present** — Alpha is at Origin, Beta is at North Station, Gamma is at East Station;
  `/npcs` at each location lists the correct NPC — observable via: `/npcs` output at Origin,
  North, East.

- **Pig Latin code-switch wired** — the `language_directive` for `x-pig-lat` includes the
  pig Latin phrase guide; the NPC system prompt contains pig Latin instructions — observable
  via: `cargo test -p parish-npc language_directive_includes_pig_lat_guide` passes.

- **Mod-list selection works** — `discover_mods_in()` with a `mod-list.toml` selecting
  `"testbed"` loads testbed; selecting `"rundale"` loads Rundale; absent `mod-list.toml`
  with two setting mods is still an error — observable via:
  `cargo test -p parish-core discover_mods` tests all pass.

- **Blueprint palette delivered to frontend** — the `UiConfigSnapshot` returned by the
  server has the correct dark-navy bg (`#0a1929`) and cyan accent (`#00d4ff`), and
  `map_overlay = "grid"` — observable via: `/api/ui-config` response in headless server
  run, or `mcp__parish__parish_world_snapshot` after boot with testbed active.

- **Blueprint CSS + grid overlay** — when `map_overlay === "grid"`, `document.body` gains
  class `blueprint-mode` (monospace fonts) and each map container renders a
  `.blueprint-grid-overlay` div — observable via: browser / screenshot after visual
  verification step.

## Verification script

Run:
```
cargo run --manifest-path parish/Cargo.toml -p parish-cli \
  -- --script parish/testing/fixtures/play_generic-testbed-mod.txt
```

Expected signals in output:
- `"location"` field shows `"Origin"` on startup
- `go north` → location name contains `"North"`
- `go east` → location name contains `"East"`
- `/npcs` at Origin → list includes `"Alpha"`
- `/npcs` at North Station → list includes `"Beta"`
- Navigation back to Origin succeeds
- No error lines in the JSON stream

Visual / browser criteria (not in CLI script):
- `map_overlay` field present in `/api/ui-config` response
- Blueprint palette colors appear in frontend
- Grid overlay visible on map panel
