Evidence type: gameplay transcript

## Summary

Resolved all 11 items from `parish/crates/parish-geo-tool/TODO.md`:

### P2 items (high priority)
- **TD-010**: Refactored `classify_element` from 125 lines to under 100 by extracting 8 tag-category helpers (`classify_historic`, `classify_amenity`, `classify_building`, `classify_natural`, `classify_waterway`, `classify_landuse`, `classify_man_made`, `classify_place`) with chained `.or_else()` dispatch.
- **TD-009**: Added 3 new test cases: `test_extract_features_filters_no_coords`, `test_extract_features_filters_unclassifiable`, `test_extract_features_deduplicates_osm_ids`.
- **TD-008**: Added `LocationType::Road` to the type list in `test_all_location_types_produce_templates`.
- **TD-006**: Extracted shared `make_feature` test helper into `src/test_utils.rs`, removing 3 near-identical copies from `lod.rs`, `descriptions.rs`, and `connections.rs`.
- **TD-007**: Consolidated `ParishFile` (output.rs) and `WorldFile` (realign_rundale_coords.rs) into a single `WorldFile` struct via shared `src/world_file_shared.rs` included by both.

### P3 items (low priority)
- **TD-005**: Removed unused `DescriptionSource::LlmPending` variant from the enum.
- **TD-004**: Removed unused `type_counts` HashMap computation in `print_summary`.
- **TD-003**: Removed dead `ResponseCache::clear` method and its test.
- **TD-002**: Removed dead `connect_curated_to_generated` function (73 lines, `#[allow(dead_code)]`).
- **TD-001**: Removed dead `filter_by_distance` function and its test from `lod.rs`.
- **TD-011**: Updated module doc in `descriptions.rs` from "three tiers" to "two tiers".

### Files changed
```
src/extract.rs          - Refactored classify_element, added 3 new tests
src/descriptions.rs     - Updated docs, removed LlmPending, added Road to test
src/lod.rs              - Removed filter_by_distance + test, unused import
src/merge.rs            - Removed connect_curated_to_generated, unused import
src/cache.rs            - Removed clear() method + test
src/output.rs           - Removed type_counts, shared WorldFile
src/bin/realign_rundale_coords.rs - Shared WorldFile via include!
src/test_utils.rs       - New shared test helper
src/world_file_shared.rs - New shared WorldFile struct
src/main.rs             - Added test_utils module
src/connections.rs      - Uses shared make_feature
TODO.md                 - Updated item statuses
```

### Verification
```
$ cargo fmt --check -p parish-geo-tool
(no output)

$ cargo clippy -p parish-geo-tool -- -D warnings
(no output)

$ cargo test -p parish-geo-tool
running 91 tests (75 main + 16 bin)
test result: ok. 91 passed; 0 failed

$ just witness-scan
Witness scan passed.
```
