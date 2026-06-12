# parish-geo-tool — agent scope

OSM data download and Parish world graph conversion. Two binaries: `parish-geo-tool` (Overpass query pipeline) and `realign_rundale_coords` (coordinate drift correction for Rundale). Full detail at the `rundale-geo-tool` skill: [`.agents/skills/rundale-geo-tool/SKILL.md`](../../../.agents/skills/rundale-geo-tool/SKILL.md). See root [`AGENTS.md`](../../../AGENTS.md) for repo-wide rules.

## Scoped commands

```sh
cargo test -p parish-geo-tool                                # unit + integration
cargo run  -p parish-geo-tool -- --area "Kiltoom"            # OSM extract pipeline
cargo run  -p parish-geo-tool -- --bbox 53.45,-8.05,53.55,-7.95
just realign-coords                                           # run realign_rundale_coords in-place
just realign-coords-run -- --baseline-world <path>            # realign from baseline delta
```

## Local gotchas

- **Nominatim is the wrong primary source for 1820s Irish geography.** Prefer historical OS maps (6-inch First Edition, ~1837). Use `--set-coord` with a known historical coordinate + `--set-source "OS 6-inch ca. 1837"` to pin a location as `Manual` so future geocode passes skip it.
- **Three coordinate modes: absolute, relative, and graph-delta.** `geo_kind: Real` locations geocode from Nominatim. `Manual` are author-pinned with `geo_source`. `relative_to` locations derive from anchor + (dnorth_m, deast_m) offset. `Fictional` locations with no `relative_to` are realigned by BFS-weighted average of nearby anchor deltas (up to 6 hops via `infer_delta`).
- **OSM data must be cached.** `cache.rs` caches Overpass responses to disk (`--cache-dir`, default `data/cache/geo`). Pass `--no-cache` to bypass reads (still writes). Entries are plain JSON keyed by normalized query hash.
- **World file indent convention is 4 spaces.** Both the pipeline and `realign_rundale_coords` write with `serde_json`'s 4-space `PrettyFormatter` — keeps `world.json` byte-identical through Designer round-trips.
- **`realign_rundale_coords` baseline mode.** Pass `--baseline-world <path>` for a pre-geocode world file; the tool computes deltas for `Real` locations and realigns connected fictionals without hitting Nominatim.
- **Cycle detection in `relative_to` resolution.** `resolve_relative_positions` detects cyclic references and missing anchor IDs and bails with a descriptive error.

## Module map

`src/main.rs` CLI entry + `AdminLevel` enum, `src/bin/realign_rundale_coords/` coordinate realignment utility (`main.rs`, `overrides.rs`, `geocode.rs`, `realign.rs` — #1200), `src/lib.rs` library surface (re-exports `osm_model`), `src/world_file_shared.inc` shared world-file serde types, `src/osm_model.rs` OSM data model types, `src/extract/` feature extraction (`mod.rs`, `classify.rs`, `dedup.rs`, `crossroads.rs` — #1200), `src/overpass.rs` Overpass query builder + HTTP client, `src/merge.rs` merge OSM extracts with hand-authored data, `src/pipeline.rs` end-to-end download→extract→connect→describe→merge→output, `src/lod.rs` level-of-detail filtering, `src/output.rs` world.json formatter, `src/descriptions.rs` location description templates, `src/connections.rs` graph edge building from road network, `src/cache.rs` HTTP response cache, `src/test_utils.rs` test helpers.
