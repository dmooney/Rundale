# TD-011: Extract inline JSON template from build_tier1_system_prompt

## Summary

Extracted the ~55-line inline JSON example embedded in the `format!()` call within `build_tier1_system_prompt()` into a named constant `EXAMPLE_RESPONSE_BLOCK`.

## Changes

**`parish/crates/parish-npc/src/lib.rs`:**
- Added `EXAMPLE_RESPONSE_BLOCK` constant (line 396) — a `&str` containing the example JSON response block previously inlined in the `format!()` string
- Removed the inline example block (lines 443-448) from the `format!()` call
- Added `prompt.push_str(EXAMPLE_RESPONSE_BLOCK);` after the `format!()` to maintain identical output

Key detail: the original used `{{`/`}}` format-escape syntax (for literal braces inside `format!()`), which was changed to plain `{`/`}` in the constant since it's no longer a format template.

**`parish/crates/parish-npc/TODO.md`:**
- Moved TD-011 from Open to Done
- Removed TD-011 from Follow-up section

## Verification

```sh
$ cargo test -p parish-npc
# 412 tests passed (400 unit + 3 gossip integration + 6 tier2 LLM + 3 doctests)

$ cargo clippy -p parish-npc --all-targets -- -D warnings
# Clean — no warnings
```
