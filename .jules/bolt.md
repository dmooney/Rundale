# Bolt's Journal

## 2026-03-31 - Single-pass fuzzy name search
**Learning:** `find_by_name_with_config` in `world/graph.rs` scanned all locations 8 separate times (one per priority level), calling `to_lowercase()` on every name/alias each time. Plus a 9th pass for fuzzy scoring. Consolidating into a single pass that caches lowercased strings eliminated ~8× redundant string allocations per lookup.
**Action:** When adding new match levels, add them to the single-pass priority chain rather than appending another full scan loop.

## 2026-03-28 - Pre-existing test breakage in inference module
**Learning:** The `max_tokens` parameter was added to `build_request()` and `InferenceQueue::send()` signatures but several tests weren't updated. This means test-only compilation failures can lurk undetected if `cargo test` isn't run regularly after API changes.
**Action:** When modifying function signatures, always grep for all call sites including test modules.

## 2026-04-14 - Recurring multi-pass HashMap scan anti-pattern in parish-npc
**Learning:** `known_roster` in `manager.rs` repeated the double-scan anti-pattern — separate `for other in self.npcs.values()` loops for home and workplace co-residency. Same shape as the earlier fuzzy-search fix in `graph.rs`. When multiple optional filters apply to the same collection, consolidating into one pass with per-NPC conjunction checks is both faster and clearer.
**Action:** When reviewing lookup/query methods against `NpcManager`/`WorldState` that iterate `.values()`, check whether nearby code iterates the same map again — if so, fold into a single pass.

## 2026-04-15 - Pre-lowercased storage invariant defeated by per-lookup re-lowercasing
**Learning:** `LongTermMemory::recall` in `parish-npc/memory.rs` re-lowercased both query and stored keywords inside an O(entries × query × entry_keywords) nested loop — yet `extract_keywords` (the only production producer) already stores everything lowercased. The `ek.to_lowercase()` inside `.any()` was pure waste; the `qk.to_lowercase()` should be hoisted above the entries loop. Called per NPC per dialogue turn via `build_enhanced_context_with_config`.
**Action:** When a "lowercase on read" pattern sits inside a loop, trace every write site of the compared field. If producers already normalise, document the invariant on the struct field and compare directly — don't re-normalise defensively inside hot loops.

## 2026-04-16 - `.collect::<String>().to_lowercase()` double-allocates
**Learning:** In `extract_keywords` (parish-npc/memory.rs), `word.chars().filter(...).collect::<String>().to_lowercase()` heap-allocates twice per word — first to build the filtered String, then to produce its lowercase copy. `chars().filter(...).flat_map(char::to_lowercase).collect()` folds both into a single allocating pass, because `char::to_lowercase()` returns an iterator of chars that `flat_map` + `collect::<String>()` consumes directly.
**Action:** When you see `chars().<something>().collect::<String>().to_lowercase()` (or `.to_uppercase()`), rewrite as `chars().<something>().flat_map(char::to_lowercase).collect()`. Produces identical output for non-Greek text (no context-sensitive final-sigma) at half the allocations.
