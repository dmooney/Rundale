# Acceptance Criteria — Stylized NLS Tile Pipeline

Task: Add an illustrated parchment tile source (`rundale-map`) for the Rundale game map,
pre-generated offline from NLS historic OS Ireland tiles via a new `parish-tile-art`
art pipeline and `seed-tiles` seeder command.

## Criteria

### AC-1 — `parish-tile-art` crate exists with parchment pipeline
- `parish/crates/parish-tile-art/` is a new leaf crate in the workspace.
- `TileArtist::from_config(ArtConfig::Parchment(_))` returns a `TileArtist`.
- `paint_tile()` on a 256×256 PNG input returns a 256×256 PNG output with
  parchment palette (warm cream background, sepia ink strokes, sage woodland, slate water).
- Pipeline is deterministic: same z/x/y inputs always produce identical output.

### AC-2 — Diffusion fallback is automatic
- `TileArtist::from_config(ArtConfig::Diffusion { endpoint: unreachable, … })` 
  falls back to parchment pipeline without panicking or returning an error.
- A `tracing::warn!` is emitted on fallback.

### AC-3 — `seed-tiles` seeder subcommand in `parish-geo-tool`
- `cargo run -p parish-geo-tool -- seed-tiles --help` succeeds.
- Existing `generate` subcommand continues to work (no regression).

### AC-4 — `rundale-map` tile source in engine defaults
- `MapConfig::default()` (via `default_tile_sources()`) includes a `rundale-map` entry.
- `TileSourceConfig` for `rundale-map` has correct `upstream_url`, `attribution`,
  `minzoom = 1`, `maxzoom = 17`, `tile_size = 256`.
- Total default tile sources count is 3 (`osm`, `historic`, `rundale-map`).

### AC-5 — CC-BY attribution and licence notice
- `docs/licenses/NLS_CC-BY_derivative.txt` exists with full derivative notice.
- `mods/rundale/mod.toml` contains a `[[data_sources]]` entry for NLS tiles.
- `TileSourceConfig.attribution` for `rundale-map` includes the NLS CC-BY attribution string.

### AC-6 — Game boots normally with modified `mods/rundale/mod.toml`
- A live game session starts, the world loads, and movement commands succeed.
- No regression in existing walkthrough test baseline.
