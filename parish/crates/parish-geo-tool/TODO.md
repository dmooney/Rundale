# parish-geo-tool — Technical Debt

## Open

| ID | Category | Severity | Description |
|----|----------|----------|-------------|
| TD-012 | Weak Tests | P2 | `extract_crossroads` (`src/extract.rs:303`) is `pub` and called from `pipeline::run` but has no unit test — junction-counting (3+ ways), missing-coord skip, and 50m proximity dedup are all unverified |
| TD-013 | Weak Tests | P2 | `merge::merge_locations` connection-target remap (`src/merge.rs:113-122`) is untested — every existing test (`test_merge_preserves_curated`, `test_merge_drops_duplicate_by_name`, `test_merge_drops_by_proximity`) builds locations with empty `connections`, so the ID-rewrite branch never fires |
| TD-014 | Weak Tests | P3 | `merge::determine_id_offset` merge-from-existing-file branch (`src/merge.rs:136-142`) is untested — only the explicit-offset and default-1 branches have coverage |
| TD-015 | Weak Tests | P3 | `realign_rundale_coords::derive_deltas_from_baseline` (`src/bin/realign_rundale_coords.rs:201-229`) and `apply_set_source_overrides` (line 164-174) have no unit tests — sibling functions (`apply_set_coord_overrides`, `parse_set_source`) are tested |
| TD-016 | Weak Tests | P3 | `pipeline::run` validation branch (`src/pipeline.rs:59-61`, "must specify either --area or --bbox") and dry-run early-return (`src/pipeline.rs:64-73`) have no test |
| TD-017 | Duplication | P3 | Earth radius constant `6_371_000.0` is duplicated: `src/osm_model.rs:211` (`EARTH_RADIUS_M`) and `src/bin/realign_rundale_coords.rs:302` (`EARTH_R_M`). Bin can't import from the parent module today; consider moving to `src/world_file_shared.inc` (already used as a shared inc) or a new shared `.inc` |
| TD-018 | Dead Code | P3 | `_from: &GeoFeature` parameter is unused in `generate_path_description` (`src/connections.rs:220`) and `generate_direct_description` (`src/connections.rs:240`). Both are private and only called locally — drop the parameter |
| TD-019 | Stale Docs | P3 | `--no-cache` CLI doc says "Skip cache and always re-download" (`src/main.rs:76-78`), but `OverpassClient::execute_query` (`src/overpass.rs:111-119`) only skips the cache *read*; it still calls `cache.put` after a successful download (line 137). Doc should say "Ignore existing cache entries" or the behavior should match the doc |
| TD-020 | Brittle Logic | P3 | `extract_crossroads` (`src/extract.rs:304-329`) counts node *appearances*, not unique ways: a closed loop (e.g. roundabout where `nodes.first() == nodes.last()`) credits the start/end node twice from a single way, and a node listed N≥3 times in one way is falsely promoted to a junction. Either dedupe per-way or document the heuristic |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Description |
|----|----------|----------|-------------|
| TD-001 | Dead Code | P3 | Removed `filter_by_distance` from `src/lod.rs` (dead, `#[allow(dead_code)]` "future use") |
| TD-002 | Dead Code | P3 | Removed `connect_curated_to_generated` from `src/merge.rs` (73-line dead function) |
| TD-003 | Dead Code | P3 | Removed `ResponseCache::clear` from `src/cache.rs` and its test |
| TD-004 | Dead Code | P3 | Removed unused `type_counts` HashMap computation in `print_summary` (output.rs) |
| TD-005 | Dead Code | P3 | Removed `DescriptionSource::LlmPending` variant (never constructed) |
| TD-006 | Duplication | P2 | Extracted shared `make_feature` test helper into `src/test_utils.rs`, removed 3 copies |
| TD-007 | Duplication | P2 | Consolidated `ParishFile`/`WorldFile` into shared `WorldFile` via `include!` |
| TD-008 | Weak Tests | P2 | Added `LocationType::Road` to `test_all_location_types_produce_templates` |
| TD-009 | Weak Tests | P2 | Added 3 tests: no-coords filter, unclassifiable filter, OSM-id dedup |
| TD-010 | Complexity | P2 | Split `classify_element` into 8 tag-category helpers (under 100 lines) |
| TD-011 | Stale Docs | P3 | Updated descriptions.rs module doc from "three tiers" to "two tiers" |
