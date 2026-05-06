# parish-npc-tool — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Complexity | P3 | `src/main.rs:373-384` | Deep nesting (4 levels) in `generate_parish` relationship generation: `for id in &npc_ids { for _ in 0..2 { if let Some(other) = npc_ids.choose(&mut rng) && other != id { … } } }`. |
| TD-002 | Weak Tests | P2 | `src/main.rs:409-453` | `list_npcs` has zero test coverage. User-facing query function with dynamic SQL construction and multiple filter paths. |
| TD-003 | Weak Tests | P2 | `src/main.rs:455-495` | `show_npc` has zero test coverage — neither the found nor the not-found error path is tested. |
| TD-004 | Weak Tests | P2 | `src/main.rs:530-550` | `edit_npc` has zero test coverage — no test for mood-only, occupation-only, both, or the "no changes provided" error path. |
| TD-005 | Weak Tests | P2 | `src/main.rs:570-589` | `elaborate_parish` has zero test coverage — batch promotion logic, LIMIT, and empty-result paths untested. |
| TD-006 | Weak Tests | P2 | `src/main.rs:660-680` | `stats` has zero test coverage — counts across tiers, empty DB, and multi-parish scenarios untested. |
| TD-007 | Weak Tests | P2 | `src/main.rs:682-726` | `export_npcs` has zero direct test coverage — only exercised indirectly through the import test helper; parish-filtered and unfiltered output formats untested. |
| TD-008 | Weak Tests | P2 | `src/main.rs:849-881` | `relationships` has zero test coverage — NPC-not-found error path and empty-relationships case untested. |
| TD-009 | Weak Tests | P2 | `src/main.rs:503-528,1165-1174` | `search_npcs` tested only indirectly via the `search_names` helper (line 1165) which calls `escape_like` directly, bypassing the actual function. LIMIT handling, output formatting, and the join-with-parish path are all untested. |
| TD-010 | Weak Tests | P2 | `src/main.rs:288-291` | `generate_world` empty-counties error path (`bail!("--counties is required")`) is never exercised in tests. |
| TD-011 | Duplication | P2 | `src/main.rs:947-1050,762-787` | `test_import_preserves_household_and_restores_sex` copies the full UPSERT SQL verbatim from `import_npcs` (lines 762-787) instead of factoring it out. Any schema change to the import path requires updating both copies. |
| TD-012 | Stale Docs | P3 | `src/main.rs:803` | Comment says `household_id` is nullable "in the schema (line 216)", but the actual `CREATE TABLE npcs` definition is at line 261. Stale line-number reference. |
| TD-013 | Stale Docs | P3 | `src/main.rs:604-606` | Inner function `with_filter` uses `///` doc comments; these are not rendered by rustdoc on non-public items. Should use `//` to avoid misleading readers. |
| TD-014 | Complexity | P3 | `src/main.rs:416-420` | `list_npcs` uses the `"WHERE 1=1"` anti-pattern with manual `push_str` for dynamic filter construction. A `Vec<&dyn ToSql>` + conditional-bind approach (or a query builder) would be less error-prone. |
| TD-015 | Data Consistency | P2 | `src/main.rs:295,324` | `generate_world` normalizes county names via `.to_lowercase()` (line 295), but `generate_parish` inserts parish names as-is (line 324). SQLite UNIQUE uses BINARY collation, so `--parish Kiltoom` and `--parish kiltoom` produce two separate parish rows that won't unify in exports or queries. |

## In Progress

*(none)*

## Done

*(none)*
