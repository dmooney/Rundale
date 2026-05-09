# parish-input — Technical Debt

## Open

| ID | Category | Severity | Description |
|----|----------|----------|-------------|
| TD-009 | Complexity | P2 | `parse_system_command` body has rebounded to 109 lines (`src/parser.rs:19-127`), exceeding the >100-line hotspot threshold. The arms for `/cloud`, `/flag`, `/weather`, `/preset`, `/provider`, `/model`, `/key`, `/spinner`, `/debug`, `/speed` etc. could be split into a dispatch table or grouped per-command helpers (similar to `parse_zero_arg_command` from TD-007). The TODO `## Done` entry for TD-007 is now stale on this point. |
| TD-010 | Duplication / Maintainability | P2 | `lib.rs` test module (`src/lib.rs:21-1493`) holds 137 tests for **all** sibling modules (`commands`, `parser`, `mention`, `intent_local`, `intent_llm`, `intent_types`). With 1493 LOC vs. the next-largest source file at 279 LOC, the file is ~5× larger than any peer and silently hides per-module test coverage. Move each `#[cfg(test)] mod tests` block into the module it exercises (`commands.rs`, `mention.rs`, `parser.rs`, `intent_local.rs`) so coverage is co-located. |
| TD-011 | Weak Tests | P2 | `/spinner` clamps duration to `SPINNER_MAX_SECS = 300` (`src/parser.rs:13,84-90`) but no test verifies the clamp. `parse_system_command("/spinner 999")` should yield `Spinner(300)`; add a test under `// --- /spinner command tests ---` in `src/lib.rs:1112+`. |
| TD-012 | Weak Tests | P2 | `/wait` (`src/parser.rs:55-58`) accepts any `u32` value with no clamp or upper bound; tested only with `15`, `60`, and the `abc` fallback (`src/lib.rs:621-625`). Add a test for very large input (e.g. `/wait 999999`) so the no-clamp behaviour is intentional and recorded, or introduce a clamp matching `SPINNER_MAX_SECS`. |
| TD-013 | Stale Comment | P3 | `src/intent_local.rs:57-60` claims "every verb in `move_phrases` should also appear here [in `move_verbs`] without the 'to ' suffix". This is inaccurate: multi-word phrases (`make my way`, `head over`, `pop over`, `nip`, `swing by`, `travel`) live in `move_phrases` only and intentionally do not appear as single-word verbs in `move_verbs`. Update the comment to describe the actual contract (single verbs in `move_verbs`; multi-word phrases in `move_phrases`, with "to"-less variants in the same list). |
| TD-014 | Maintainability | P2 | `src/lib.rs:1143-1216` — the `test_parse_category_all_show_and_set` table-driven test hardcodes a `cases` slice of `InferenceCategory` variants and warns in a comment that "If a new category is added to `InferenceCategory::ALL` the compiler will NOT remind you to add tests here — keep the ALL_CATS slice in sync manually." `InferenceCategory::ALL` is already used elsewhere in the workspace (see `parish-cli/src/config.rs:127`, `parish-core/src/ipc/config.rs:204`); iterate it directly so new variants are auto-covered. |
| TD-015 | Magic Number | P3 | `src/mention.rs:63` uses `rest.splitn(20, ' ')` to cap the parsed name length at 20 words. The 20 is undocumented and untested — there is no test asserting behaviour at the boundary, and no constant naming the limit. Either lift to a named `const MAX_MENTION_NAME_WORDS: usize = 20;` with a comment explaining why, or replace with `rest.split(' ')` since the loop already bails on the first lowercase/punctuated word. |
| TD-016 | Stale Docs | P3 | `README.md:11-14` (Responsibilities) lists "parse slash commands", "route natural-language input", and "return typed command/intent values" but never mentions `@mention` extraction, despite `extract_mention` / `MentionExtraction` being public exports (`src/lib.rs:18`) consumed by `parish-cli/src/headless.rs:757`. Add a bullet for mention extraction. |

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
