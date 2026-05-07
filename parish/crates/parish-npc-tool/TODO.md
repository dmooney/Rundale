# parish-npc-tool — Technical Debt

## Open

*(none)*

## In Progress

*(none)*

## Done

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Complexity | P3 | `src/main.rs:373-384` | Flattened 4-level deep nesting in `generate_parish` relationship generation by collecting pairs first, then inserting. |
| TD-002 | Weak Tests | P2 | `src/main.rs:409-453` | Added `list_npcs` tests: unfiltered, parish-filtered, occupation-filtered, tier-filtered, all-filters, empty-result (6 tests). |
| TD-003 | Weak Tests | P2 | `src/main.rs:455-495` | Added `show_npc` tests: found and not-found error paths (2 tests). |
| TD-004 | Weak Tests | P2 | `src/main.rs:530-550` | Added `edit_npc` tests: mood-only, occupation-only, both, and "no changes provided" error path (4 tests). |
| TD-005 | Weak Tests | P2 | `src/main.rs:570-589` | Added `elaborate_parish` tests: basic batch, empty-result, and zero-limit (3 tests). |
| TD-006 | Weak Tests | P2 | `src/main.rs:660-680` | Added `stats` tests: with data and empty DB (2 tests). |
| TD-007 | Weak Tests | P2 | `src/main.rs:682-726` | Added `export_npcs` tests: unfiltered, parish-filtered, empty-result (3 tests). |
| TD-008 | Weak Tests | P2 | `src/main.rs:849-881` | Added `relationships` tests: NPC found and NPC-not-found error (2 tests). |
| TD-009 | Weak Tests | P2 | `src/main.rs:503-528,1165-1174` | Added `search_npcs` direct tests: matches, no-matches, zero-limit (3 tests). |
| TD-010 | Weak Tests | P2 | `src/main.rs:288-291` | Added test for `generate_world` empty-counties error path (1 test). |
| TD-011 | Duplication | P2 | `src/main.rs:947-1050,762-787` | Extracted `import_npcs_inner` helper; test calls it instead of duplicating UPSERT SQL. |
| TD-012 | Stale Docs | P3 | `src/main.rs:803` | Fixed stale line-number reference (216 -> 261). |
| TD-013 | Stale Docs | P3 | `src/main.rs:604-606` | Changed `///` doc comments to `//` on non-public `with_filter` inner function. |
| TD-014 | Complexity | P3 | `src/main.rs:416-420` | Replaced `WHERE 1=1` anti-pattern with dynamic clauses Vec + conditional `WHERE` prefix. |
| TD-015 | Data Consistency | P2 | `src/main.rs:295,324` | Normalized parish names via `.to_lowercase()` at all insert and query sites, matching county behavior. |

2026-05-07: All 15 items resolved. 36 tests pass, clippy clean.
