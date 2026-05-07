# parish-input — Technical Debt

## Open

*(none — discovery scan results below)*

A discovery scan of the crate was performed after all original items were resolved. No new credible technical debt was found. Key observations:

- **Dead code**: All dependencies (`parish-types`, `parish-config`, `parish-inference`, `serde`) are actively used. No unused imports or dead functions detected.
- **Duplication**: The `move_phrases`/`move_verbs` duplication (TD-002) was resolved via `try_move_prefix` helper. Any remaining duplication in `parse_flag_subcommand` between "enable" and "disable" branches is acceptably minimal (~4 lines each).
- **Test coverage**: Previously untested areas (`validate_flag_name`, `/flag`, music aliases, `/weather`, `move pub`) now all have tests. Total test count increased from 119 to 137.
- **Complexity**: The `parse_system_command` match body is now under 100 lines thanks to `parse_zero_arg_command` extraction.
- **Comments/docs**: No stale or outdated comments found. File-level doc comments accurately describe module purpose.

## In Progress

*(none)*

## Done

| ID | Category | Severity | Description |
|----|----------|----------|-------------|
| TD-001 | Dead Code | P2 | Removed unused `anyhow` dependency from `Cargo.toml:14` |
| TD-002 | Duplication | P2 | Extracted shared `try_move_prefix` helper from duplicated `move_phrases`/`move_verbs` loops in `src/intent_local.rs:89-130` |
| TD-003 | Weak Tests | P1 | Added 5 tests for `validate_flag_name` covering empty, valid, max length, too long, and invalid chars |
| TD-004 | Weak Tests | P1 | Added 7 tests for `/flag` commands: bare, list, enable, disable, invalid subcommand, invalid name, `/flags` alias |
| TD-005 | Weak Tests | P2 | Added 2 tests for music session aliases (`/tune`, `/music`, `/fiddle`, `/seisiun`) and case insensitivity |
| TD-006 | Weak Tests | P2 | Added 3 tests for `/weather`: bare (show), set, and case insensitivity |
| TD-007 | Complexity | P2 | Extracted `parse_zero_arg_command` from `parse_system_command` match body, reducing it below 100 lines |
| TD-008 | Duplication | P3 | Added `"move "` to `move_verbs` so bare `move pub` (without "to") matches locally; added `test_local_parse_move_bare` test |
