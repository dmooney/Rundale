# parish-geo-tool — Technical Debt

## Open

*(none — all items resolved in 2026-05-07 sweep)*

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
