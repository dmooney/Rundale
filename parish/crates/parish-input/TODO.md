# parish-input — Technical Debt

## Open

| ID     | Category       | Severity | Location                                                | Description                                                                                                                                                                                                                                                                                                    |
| ------ | -------------- | -------- | ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| TD-017 | Complexity     | P2       | `src/parser.rs:1-1208`                                  | `parser.rs` remains a broad command parser with command dispatch, every per-command parser, branch/flag validation use, and a large inline test module. Split command families (`save`, `inference`, `provider`, `world`, `debug`) and move parser tests with their helpers before adding more slash commands. |
| TD-018 | Stale Comments | P3       | `src/intent_local.rs:19`, `src/intent_local.rs:491-555` | Inline `TODO #41/#46/#53` comments document fixed first-person movement regressions, but they are not this crate's TD IDs and look like open debt during scans. Convert to issue/test wording or local TD references during adjacent edits.                                                                    |

## In Progress

_(none)_

## Done

| ID     | Category                    | Severity | Description                                                                                                                                                  |
| ------ | --------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| TD-001 | Dead Code                   | P2       | Removed unused `anyhow` dependency from `Cargo.toml:14`                                                                                                      |
| TD-002 | Duplication                 | P2       | Extracted shared `try_move_prefix` helper from duplicated `move_phrases`/`move_verbs` loops in `src/intent_local.rs:89-130`                                  |
| TD-003 | Weak Tests                  | P1       | Added 5 tests for `validate_flag_name` covering empty, valid, max length, too long, and invalid chars                                                        |
| TD-004 | Weak Tests                  | P1       | Added 7 tests for `/flag` commands: bare, list, enable, disable, invalid subcommand, invalid name, `/flags` alias                                            |
| TD-005 | Weak Tests                  | P2       | Added 2 tests for music session aliases (`/tune`, `/music`, `/fiddle`, `/seisiun`) and case insensitivity                                                    |
| TD-006 | Weak Tests                  | P2       | Added 3 tests for `/weather`: bare (show), set, and case insensitivity                                                                                       |
| TD-007 | Complexity                  | P2       | Extracted `parse_zero_arg_command` from `parse_system_command` match body, reducing it below 100 lines                                                       |
| TD-008 | Duplication                 | P3       | Added `"move "` to `move_verbs` so bare `move pub` (without "to") matches locally; added `test_local_parse_move_bare` test                                   |
| TD-009 | Complexity                  | P2       | Split `parse_system_command` into dispatch table with per-command helpers in `src/parser.rs`                                                                 |
| TD-010 | Duplication/Maintainability | P2       | Moved 137 tests from `lib.rs` into the files they exercise (`parser.rs`, `intent_local.rs`, `commands.rs`, `mention.rs`, `intent_types.rs`, `intent_llm.rs`) |
| TD-011 | Weak Tests                  | P2       | Added `/spinner` clamp test (`test_parse_spinner_clamped_to_max`) verifying values above 300s are capped                                                     |
| TD-012 | Weak Tests                  | P2       | Added `/wait` large input test (`test_parse_wait_large_input_fallback`) verifying u32 overflow falls back to default                                         |
| TD-013 | Stale Comment               | P3       | Fixed `move_verbs`/`move_phrases` comment in `src/intent_local.rs` to accurately describe their relationship                                                 |
| TD-014 | Maintainability             | P2       | Replaced hardcoded `InferenceCategory` slice in test with direct iteration over `InferenceCategory::ALL`                                                     |
| TD-015 | Magic Number                | P2       | Named `MAX_MENTION_NAME_WORDS` constant in `src/mention.rs` (was literal `20` in `splitn`)                                                                   |
| TD-016 | Stale Docs                  | P3       | Added `@mention` extraction to `README.md` responsibilities list                                                                                             |

## Progress Log

- **2026-05-11**: Completed TD-009 through TD-016. All 139 unit tests, 6 integration tests, and 1 doctest pass. `cargo clippy -p parish-input` and `cargo fmt -p parish-input` clean.
- **2026-05-25**: Refreshed the debt scan against current source. Reopened TD-017 and TD-018 for the parser hotspot and historical TODO anchors.
