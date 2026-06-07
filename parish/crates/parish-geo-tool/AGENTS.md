# parish-geo-tool — agent scope

OSM data download + Parish world graph conversion. Two binaries: the main `parish-geo-tool` (Overpass query pipeline) and `realign_rundale_coords` (coordinate drift correction for Rundale). Documented in depth by the `rundale-geo-tool` agent skill at [`.agents/skills/rundale-geo-tool/SKILL.md`](../../../.agents/skills/rundale-geo-tool/SKILL.md). See root [`AGENTS.md`](../../../AGENTS.md) for repo-wide rules.

## Scoped commands

```sh
cargo test -p parish-geo-tool                                # unit + integration
cargo run  -p parish-geo-tool -- --area "Kiltoom"            # OSM extract pipeline
cargo run  -p parish-geo-tool -- --bbox 53.45,-8.05,53.55,-7.95
just realign-coords                                           # run realign_rundale_coords in-place
just realign-coords-run -- --baseline-world <path>            # realign from baseline delta
```

## Local gotchas

- **Nominatim alone is the wrong primary source for 1820s Irish geography.** Prefer historical OS maps (6-inch First Edition, ~1837) for real-world pinning. Nominatim returns modern coordinates; buildings, roads, and coastlines have shifted. Use `--set-coord` with a known historical coordinate + `--set-source "OS 6-inch ca. 1837"` to pin a location and mark it `Manual` so future geocode passes skip it.
- **Three coordinate modes: absolute, relative, and graph-delta.** `geo_kind: Real` locations get geocoded from Nominatim. `Manual` locations are author-pinned with a provenance `geo_source`. `relative_to` locations derive from an anchor + (dnorth_m, deast_m) offset. `Fictional` locations with no `relative_to` get realigned by a BFS-weighted average of nearby anchor deltas — the `infer_delta` algorithm walks the connection graph up to 6 hops.
- **OSM data must be cached.** The `cache.rs` module caches Overpass responses to disk (`--cache-dir`, default `data/cache/geo`). Avoids hammering the Overpass API during iterative development. Pass `--no-cache` to bypass reads (still writes). Cache entries are plain JSON keyed by normalized query hash.
- **World file indent convention is 4 spaces.** Both the pipeline output and `realign_rundale_coords` write with `serde_json`'s 4-space `PrettyFormatter`. This keeps `world.json` byte-identical through Designer editor round-trips.
- **`realign_rundale_coords` has a baseline mode.** Pass `--baseline-world <path>` pointing to a pre-geocode world file. The tool computes deltas for `Real` locations (excluding `Fictional` and `relative_to`), applies them to the current world, and realigns connected fictionals. Useful when you want to apply known drifts without hitting Nominatim again.
- **Cycle detection in `relative_to` resolution.** The `resolve_relative_positions` function detects cyclic references and bails with a descriptive error. It also catches references to missing anchor IDs.

## Module map

`src/main.rs` CLI entry + `AdminLevel` enum, `src/bin/realign_rundale_coords/` coordinate realignment utility (`main.rs` CLI+orchestration, `overrides.rs`/`geocode.rs`/`realign.rs` submodules — #1200), `src/lib.rs` library surface (re-exports `osm_model`), `src/world_file_shared.inc` shared world-file serde types, `src/osm_model.rs` OSM data model types, `src/extract/` feature extraction from Overpass responses (`mod.rs` POI pass, `classify.rs`/`dedup.rs`/`crossroads.rs` submodules — #1200), `src/overpass.rs` Overpass API query builder + HTTP client, `src/merge.rs` merge OSM extracts with hand-authored data, `src/pipeline.rs` end-to-end download→extract→connect→describe→merge→output workflow, `src/lod.rs` level-of-detail density filtering, `src/output.rs` world.json output formatting, `src/descriptions.rs` location description template generation, `src/connections.rs` graph edge/connection building from road network, `src/cache.rs` HTTP response caching, `src/test_utils.rs` test helpers.
