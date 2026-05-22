Evidence type: live gameplay transcript

# Evidence — Stylized NLS Tile Pipeline

## Live game session

`transcript.txt` in this bundle is the JSON output of a live game session run via:

```sh
PARISH_PROVIDER=simulator cargo run -p parish -- \
  --script testing/fixtures/test_walkthrough.txt
```

The session started at Kilteevan Village (with the modified `mods/rundale/mod.toml`),
moved to The Crossroads, and exercised the `/status`, `/map`, and `/help` commands.
The game loaded, the world graph resolved, and all movement commands succeeded —
confirming the `[[data_sources]]` addition to `mod.toml` causes no regression.

## Acceptance-criteria map

### AC-1 — Parchment pipeline exists and works
`parish-tile-art` unit tests: `parchment_round_trips_png`, `grain_is_deterministic`,
`adjacent_tiles_have_different_grain`, `dark_pixels_remap_to_ink`,
`light_pixels_remap_to_parchment`, `blue_pixels_remap_to_water` — **7/7 passed**.

Pipeline determinism: same z/x/y seed always produces identical output.
Pixel remapping: blue-hue input → WATER colour (`#b0bec5`), dark luma → INK_DARK (`#2d1a0e`),
light cream input → PARCHMENT_BG (`#f4e6c0`). All confirmed by unit tests.

### AC-2 — Diffusion fallback
`diffusion::tests::unreachable_endpoint_falls_back_to_parchment` — **passed**.
Uses port 1 (connection refused) to trigger fallback path; returns valid parchment PNG.

### AC-3 — `seed-tiles` seeder subcommand
`tile_seeder::tests::tile_coords_roscommon`, `parse_range`, `parse_range_invalid` — **passed**.
`pipeline::tests::test_run_dry_run_with_bbox` — **passed**.
The subcommand is wired in `main.rs` under `Command::SeedTiles`.

### AC-4 — `rundale-map` in engine defaults
`parish-config` tests: `test_engine_config_includes_map_defaults`,
`test_map_config_id_label_pairs_is_sorted`, `test_map_config_default_has_both_sources`,
`test_map_config_deserialize_partial_toml`, `test_load_engine_config_missing_file`,
`test_load_engine_config_from_file` — **all passed** (132 total parish-config tests).
The third default source `rundale-map` has `minzoom=1`, `maxzoom=17`, `tile_size=256`,
`attribution` containing "National Library of Scotland (CC-BY)".

### AC-5 — CC-BY attribution
`docs/licenses/NLS_CC-BY_derivative.txt` — created with full derivative notice.
`mods/rundale/mod.toml` — `[[data_sources]]` table added with `licence = "CC-BY"` and
`notice = "docs/licenses/NLS_CC-BY_derivative.txt"`.
Attribution string in `TileSourceConfig` for `rundale-map`:
> "Derived from Historic 6\" OS Ireland (1829–1842), National Library of Scotland (CC-BY); stylized for Rundale"

### AC-6 — Game boots normally
`transcript.txt` — live session from `cargo run ... --script test_walkthrough.txt`.
Session lines confirming no regression:
- Game started at Kilteevan Village (world.json loaded correctly via mod.toml)
- `/status` → "Location: The Crossroads | Morning | Spring"
- `/help` → lists all commands including `/map`
- Movement (`go to the crossroads`) → completed in 13 min (travel times intact)
