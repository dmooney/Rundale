# Bolt's Journal

## 2026-03-31 - Single-pass fuzzy name search
**Learning:** `find_by_name_with_config` in `world/graph.rs` scanned all locations 8 separate times (one per priority level), calling `to_lowercase()` on every name/alias each time. Plus a 9th pass for fuzzy scoring. Consolidating into a single pass that caches lowercased strings eliminated ~8× redundant string allocations per lookup.
**Action:** When adding new match levels, add them to the single-pass priority chain rather than appending another full scan loop.

## 2026-03-28 - Pre-existing test breakage in inference module
**Learning:** The `max_tokens` parameter was added to `build_request()` and `InferenceQueue::send()` signatures but several tests weren't updated. This means test-only compilation failures can lurk undetected if `cargo test` isn't run regularly after API changes.
**Action:** When modifying function signatures, always grep for all call sites including test modules.
