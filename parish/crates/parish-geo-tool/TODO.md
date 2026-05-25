# parish-geo-tool — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-021 | Complexity | P2 | `src/extract.rs:1-954` | Largest library file. It combines Overpass element filtering, tag classification, road/path extraction, geometry conversion, crossroads detection, and tests. Split into classifier, geometry extraction, crossroads, and test-fixture modules before adding more OSM feature handling. |
| TD-022 | Complexity | P2 | `src/bin/realign_rundale_coords.rs:1-863` | The realignment binary carries CLI parsing, graph resolution, baseline delta inference, relative-position solving, override parsing, output rewriting, and tests in one file. Extract reusable realignment logic into the library so the bin stays a thin CLI wrapper. |
| TD-023 | API Robustness | P3 | `src/connections.rs:165` | `connect_nearby` compares against `best.unwrap()` inside a conditional. It is currently guarded by `best.is_none() ||`, but the unwrap makes the invariant easy to break during future edits. Replace with `match best`/`Option::is_none_or` style in a small cleanup. |

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
| TD-012 | Weak Tests | P2 | Added 5 unit tests for `extract_crossroads` (empty, 2-way, 3-way, repeated-node dedup, no-geometry) |
| TD-013 | Weak Tests | P2 | Added `test_merge_remaps_generated_connection_targets` for connection-target ID remap |
| TD-014 | Weak Tests | P2 | Added `test_determine_id_offset_from_existing_file` using tempfile + serde_json |
| TD-015 | Weak Tests | P2 | Added 4 tests for `derive_deltas_from_baseline` and `apply_set_source_overrides` in realign binary |
| TD-016 | Weak Tests | P2 | Added 3 async tests for `pipeline::run` dry-run and input-validation branches |
| TD-017 | Duplication | P2 | Extracted shared `pub const EARTH_RADIUS_M` to `osm_model.rs`; created `lib.rs` for cross-binary reuse |
| TD-018 | Dead Code | P3 | Removed unused `_from` parameter from `generate_path_description` and `generate_direct_description` |
| TD-019 | Stale Docs | P3 | Fixed `--no-cache` CLI doc from "always re-download" to "skip reading from cache" |
| TD-020 | Brittle Logic | P2 | Changed `extract_crossroads` to count unique ways per node (HashSet) instead of raw appearances |

## Progress Log

- **2026-05-11**: Resolved TD-012 through TD-020. All fixes behavior-safe; 22 new tests added. `cargo test -p parish-geo-tool` passes (113 tests). `cargo clippy -p parish-geo-tool --all-targets` clean.
- **2026-05-25**: Refreshed the debt scan against current source. Reopened TD-021 through TD-023 for current layout hotspots and a small brittle conditional.
