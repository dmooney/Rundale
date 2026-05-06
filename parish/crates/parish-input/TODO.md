# parish-input — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P2 | `Cargo.toml:14` | Unused dependency `anyhow` — listed in `[dependencies]` but never imported or referenced in any `.rs` source file in this crate |
| TD-002 | Duplication | P2 | `src/intent_local.rs:89-108` and `src/intent_local.rs:111-130` | Identical byte-offset computation and `PlayerIntent` construction logic duplicated between the `move_phrases` loop and the `move_verbs` loop. The only difference is the prefix source (`move_phrases` vs `move_verbs`). Extract into a shared helper `try_move_prefix(trimmed, lower, prefix, raw_input) -> Option<PlayerIntent>` |
| TD-003 | Weak Tests | P1 | `src/commands.rs:178-196` | `validate_flag_name` has zero unit tests — no coverage for empty input, boundary length (64 chars), invalid characters, or valid flag name paths. Unlike `validate_branch_name` which has 6 test functions |
| TD-004 | Weak Tests | P1 | `src/parser.rs:122-127` | `/flag` command family has zero positive tests — no coverage for `/flag` bare, `/flag list`, `/flag enable <name>`, `/flag disable <name>`, `/flag enable` (bare, shows list), `/flag disable` (bare, shows list), or `/flag bogus` (invalid subcommand → `InvalidFlagName`) |
| TD-005 | Weak Tests | P2 | `src/parser.rs:50-52` | Music session aliases (`/tune`, `/music`, `/fiddle`, `/seisiun`) have zero positive tests confirming they map to `Command::Session`. Only `/session start` is tested (negatively, as trailing-text rejection) |
| TD-006 | Weak Tests | P2 | `src/parser.rs:119-120` | `/weather` command has zero tests — no coverage for bare `/weather` (show current) or `/weather <name>` (set weather) |
| TD-007 | Complexity | P2 | `src/parser.rs:31-141` | `parse_system_command` match body spans 111 lines (exceeds 100-line threshold). Contains 15 logical groups (zero-arg commands, fork, load, map, wait, theme, unexplored, preset, provider, model, key, spinner, debug, speed, cloud, weather, flag, category, default). Could be split into `parse_zero_arg_command`, `parse_arg_command`, `parse_category_command` dispatch functions |
| TD-008 | Duplication | P3 | `src/intent_local.rs:18-55` and `src/intent_local.rs:57-86` | `move_verbs` list is a manually-maintained subset of `move_phrases` with "to " stripped. The verb `"move"` is present in `move_phrases` as `"move to "` but absent from `move_verbs`, so bare `"move pub"` (without `"to"`) falls through to LLM fallback instead of local match |

## In Progress

*(none)*

## Done

*(none)*
