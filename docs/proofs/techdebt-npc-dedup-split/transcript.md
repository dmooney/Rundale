# Techdebt: parish-npc dedup and split (TD-002 + TD-010)

## What was changed and why

### TD-002 (Duplication) — Replace local `make_test_npc`/`test_npc` with `test_helpers::make_test_npc`

The `ticks.rs` test module defined its own `make_test_npc(id, name, location)` helper duplicating all Npc field construction (~28 lines). `reactions.rs` defined `test_npc(id, name, occupation, workplace)` with custom defaults. Both duplicated the structural initialization that `test_helpers::make_test_npc` already provides.

Changes:
- `ticks.rs`: Replaced the local `make_test_npc` body to delegate to `crate::test_helpers::make_test_npc` and override only the module-specific defaults (name, age=40, personality="Friendly").
- `reactions/arrival_reactions.rs` tests: Changed `test_npc` to delegate to `crate::test_helpers::make_test_npc` and override fields.
- `reactions/emoji_reactions.rs` tests: Changed `test_npc` to delegate to `crate::test_helpers::make_test_npc` and override fields.

### TD-010 (Complexity) — Split 2,017-line `reactions.rs` into three files

The monolithic `reactions.rs` contained three distinct subsystems:
1. Shared palette (`REACTION_PALETTE`, `ReactionLog`) 
2. Emoji keyword/LLM reactions
3. Arrival reactions + LLM greeting

Split:
- `reactions.rs` (~170 lines): Shared palette + ReactionLog, declares sub-modules, re-exports public API
- `reactions/emoji_reactions.rs` (~230 lines): Keyword reactions, LLM-informed reactions
- `reactions/arrival_reactions.rs` (~850 lines): Arrival reaction templates and algorithm, LLM greeting

Public API preserved — all re-exports ensure `parish_npc::reactions::*` paths remain identical.

### Before/After line counts

| File | Before | After |
|------|--------|-------|
| `reactions.rs` | 2,017 lines | ~170 lines |
| `reactions/emoji_reactions.rs` | — | ~230 lines (new) |
| `reactions/arrival_reactions.rs` | — | ~850 lines (new) |
| `ticks.rs` | 2,158 lines | ~2,140 lines |
| `TODO.md` | 39 lines | ~30 lines |

### Files changed
- `parish/crates/parish-npc/src/reactions.rs` — rewritten as parent module with re-exports
- `parish/crates/parish-npc/src/reactions/emoji_reactions.rs` — created
- `parish/crates/parish-npc/src/reactions/arrival_reactions.rs` — created
- `parish/crates/parish-npc/src/ticks.rs` — test helper delegated to shared
- `parish/crates/parish-npc/TODO.md` — moved TD-002, TD-010 to Done; removed Follow-up

### Commands run

```sh
cargo test -p parish-npc       # 400 passed, 6 integration, 3 doc-tests
cargo clippy -p parish-npc --all-targets -- -D warnings  # clean
```
