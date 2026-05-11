Evidence type: gameplay transcript

## Summary

Resolved TD-012 through TD-020 in `parish/crates/parish-geo-tool/TODO.md`, completing all open technical-debt items for the crate.

### TD-012 — Weak Tests: extract_crossroads
Added 5 unit tests covering:
- Empty road response returns empty crossroads
- 2-way junction (node shared by 2 ways) is NOT a crossroads
- 3-way junction produces exactly one crossroads with correct metadata
- Repeated node within the same way does NOT inflate the way count
- Ways without geometry are skipped due to missing coordinates

### TD-013 — Weak Tests: merge connection-target remap
Added `test_merge_remaps_generated_connection_targets`:
- Creates generated locations with connections to each other
- Verifies that after merge, generated IDs are reassigned (100→2, 101→3)
- Verifies connection targets are remapped to the new IDs

### TD-014 — Weak Tests: determine_id_offset from existing file
Added `test_determine_id_offset_from_existing_file`:
- Writes a temporary JSON file with two locations (ids 5 and 12)
- Calls `determine_id_offset(Some(path), None)`
- Asserts result is `13` (max_id + 1)

### TD-015 — Weak Tests: realign_rundale_coords
Added 4 tests:
- `apply_set_source_overrides_sets_geo_source`: verifies `geo_source` field is set
- `apply_set_source_overrides_fails_on_missing_name`: verifies error on unknown location
- `derive_deltas_from_baseline_computes_shifts`: verifies delta computation for moved real locations
- `derive_deltas_from_baseline_ignores_fictional_and_relative`: verifies fictional and `relative_to` locations are excluded from delta calculation

### TD-016 — Weak Tests: pipeline dry-run and validation
Added 3 async tests for `pipeline::run`:
- `test_run_dry_run_with_area`: dry-run succeeds without network
- `test_run_dry_run_with_bbox`: dry-run with bbox succeeds without network
- `test_run_fails_without_area_or_bbox`: returns descriptive error when neither is provided

Also added 2 negative tests for `output::validate_output`:
- `test_validate_output_fails_on_invalid_json`
- `test_validate_output_fails_on_broken_connections`

### TD-017 — Duplication: Earth radius constant
- Created `src/lib.rs` exposing `pub mod osm_model`
- Made `EARTH_RADIUS_M` a `pub const` in `osm_model.rs`
- Updated `realign_rundale_coords.rs` to import `EARTH_RADIUS_M` from `parish_geo_tool::osm_model`
- Removed the duplicate local `const EARTH_R_M` from `realign_rundale_coords.rs`

### TD-018 — Dead Code: unused _from parameters
- Removed `_from: &GeoFeature` from `generate_path_description` and `generate_direct_description`
- Updated all call sites in `generate_connections` and `ensure_connectivity`
- Updated existing tests to match new signatures

### TD-019 — Stale Docs: --no-cache CLI doc
- Fixed doc string from "Skip cache and always re-download" to "Skip reading from cache (responses are still written to cache)."

### TD-020 — Brittle Logic: extract_crossroads counts appearances
- Changed `extract_crossroads` to use `HashMap<i64, HashSet<i64>>` tracking unique way IDs per node
- In the geometry branch, deduplicates repeated nodes within a single way using a `seen` HashSet
- In the nodes-only branch, deduplicates using `unique_nodes: HashSet<i64>`
- Junction threshold (`>= 3`) now correctly means "3+ unique ways" not "3+ appearances"

### Files changed
```
src/lib.rs                              - New library root for cross-binary module reuse
src/osm_model.rs                        - Made EARTH_RADIUS_M pub
src/bin/realign_rundale_coords.rs       - Import shared EARTH_RADIUS_M; add 4 tests
src/extract.rs                          - Fix unique-way counting; add 5 crossroads tests
src/connections.rs                      - Remove unused _from parameters
src/main.rs                             - Fix --no-cache doc
src/merge.rs                            - Add connection remap and id-offset tests
src/pipeline.rs                         - Add dry-run and validation tests
src/output.rs                           - Add negative validation tests
TODO.md                                 - Move TD-012–TD-020 to Done; add progress log
```

### Verification
```
$ cd parish
$ cargo fmt -p parish-geo-tool
(no output)

$ cargo clippy -p parish-geo-tool --all-targets
(no output)

$ cargo test -p parish-geo-tool
running 113 tests (6 lib + 87 main + 20 realign)
test result: ok. 113 passed; 0 failed; 0 ignored
```
