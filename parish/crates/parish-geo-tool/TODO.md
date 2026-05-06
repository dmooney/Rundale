# parish-geo-tool — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P3 | `src/lod.rs:40-61` | `filter_by_distance` function marked `#[allow(dead_code)]` — public API is fully implemented and tested but never called from crate body (only tests). Intended for "future use" per comment at line 41. |
| TD-002 | Dead Code | P3 | `src/merge.rs:152-225` | `connect_curated_to_generated` function marked `#[allow(dead_code)]` — 73-line public API never invoked by any pipeline stage. Intended "for curated-to-generated linking (future use)". |
| TD-003 | Dead Code | P3 | `src/cache.rs:54-65` | `ResponseCache::clear` method marked `#[allow(dead_code)]` — "Public API for manual cache management" never called from any CLI flag or pipeline path. |
| TD-004 | Dead Code | P3 | `src/output.rs:154-163` | `print_summary` builds a `type_counts` HashMap (9 lines of logic) that is never read, printed, or returned. Pure waste compute. |
| TD-005 | Dead Code | P3 | `src/descriptions.rs:19` | `DescriptionSource::LlmPending` variant is defined but never constructed by any code path in this crate. The variant exists as a placeholder with no producer. |
| TD-006 | Duplication | P2 | `src/lod.rs:109-121` `src/descriptions.rs:395-407` `src/connections.rs:346-357` | `make_feature` test helper duplicated across three modules with near-identical bodies. Any schema change to `GeoFeature` requires touching each copy. Extract a common `parish_geo_test::test_utils` module or use a shared fixture. |
| TD-007 | Duplication | P2 | `src/output.rs:48-51` `src/bin/realign_rundale_coords.rs:42-45` | `ParishFile` (lib) and `WorldFile` (bin) are structurally identical (`{ locations: Vec<LocationData> }`). Both read/write the same JSON format. Consolidate into a single shared type. |
| TD-008 | Weak Tests | P2 | `src/descriptions.rs:469-503` | `test_all_location_types_produce_templates` exhaustively lists 24 `LocationType` variants but omits `LocationType::Road` — if `Road` never produces a valid template, the test would not catch it. |
| TD-009 | Weak Tests | P2 | `src/extract.rs:493-536` | `test_extract_features_filters_unnamed` only covers the unnamed-building-gets-filtered path. Missing are (a) elements with `lat=None` / `lon=None`, (b) elements where `classify_element` returns `None`, and (c) elements classified as `LocationType::Road` (which should be skipped). |
| TD-010 | Complexity | P2 | `src/extract.rs:83-207` | `classify_element` is 125 lines — exceeds the 100-line threshold. Contains a long chain of `if let Some(foo) = tags.get(...)` checks (8 tag categories). Each branch is simple but the accumulation makes the function hard to scan for exhaustiveness. |
| TD-011 | Stale Docs/Comments | P3 | `src/descriptions.rs:4-6` | Module doc says "Supports three tiers: curated, template, and llm (to be populated by a future LLM enrichment pass)" but no code path in this crate ever produces `LlmPending`. The docs describe aspirational behavior, not current behavior. |

## In Progress

*(none)*

## Done

*(none)*
