Evidence type: gameplay transcript

# Transcript: parish-npc-tool techdebt sweep

## Changes made

All changes in `parish/crates/parish-npc-tool/src/main.rs` (15 items):

### P2 items
- **TD-010**: Added `test_generate_world_rejects_empty_counties` — verifies `--counties is required` error path
- **TD-011**: Extracted `import_npcs_inner` helper, eliminating UPSERT SQL duplication between `import_npcs` and `test_import_preserves_household_and_restores_sex`
- **TD-015**: Normalized parish names at all insert and query sites via `.to_lowercase()`, matching county behavior. Affects `generate_parish`, `list_npcs`, `elaborate_parish`, `validate_db`, `export_npcs`, `import_npcs`
- **TD-002**: 6 tests for `list_npcs` (unfiltered, parish-filtered, occupation-filtered, tier-filtered, all-filters, empty-result)
- **TD-003**: 2 tests for `show_npc` (found, not-found)
- **TD-004**: 4 tests for `edit_npc` (mood-only, occupation-only, both, no-changes error)
- **TD-005**: 3 tests for `elaborate_parish` (basic batch, empty-result, zero-limit)
- **TD-006**: 2 tests for `stats` (with data, empty DB)
- **TD-007**: 3 tests for `export_npcs` (unfiltered, parish-filtered, empty-result)
- **TD-008**: 2 tests for `relationships` (NPC found, NPC not-found)
- **TD-009**: 3 tests for `search_npcs` (matches, no-matches, zero-limit)

### P3 items
- **TD-001**: Flattened 4-level nesting in `generate_parish` relationship loop by collecting pairs first
- **TD-012**: Fixed stale line-number reference (216 -> 261)
- **TD-013**: Changed `///` to `//` on non-public `with_filter` inner function
- **TD-014**: Replaced `WHERE 1=1` anti-pattern with dynamic clauses Vec + conditional `WHERE` prefix

## Verification

### cargo test
```
running 36 tests
test result: ok. 36 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### cargo clippy (deny warnings)
Clean — no warnings.

### cargo fmt --check
Clean — no formatting issues.

### just agent-check
Passed after proof bundle created.

### just witness-scan
Clean.
