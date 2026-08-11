# parish-input — agent scope

Player input parsing and command interpretation for the Parish engine. Backend-agnostic leaf crate — consumed by every entry point via `parish-core`. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p parish-input                       # unit tests
cargo test -p parish-input --test '*'            # integration (llm fallback)
```

## Local gotchas

- **Leaf-crate dependency rule.** Depends only on `parish-types`, `parish-config`, `parish-inference`. Never take a dependency on `parish-core` or any runtime crate (tauri, axum, engine).
- **LLM is the fallback, not the primary path.** `intent_local.rs` + `parser.rs` handle all known command forms (movement, look/examine, and physical interaction narration) first. `intent_llm.rs` is only invoked for unrecognised input — keep local coverage broad to avoid unnecessary inference calls. Parish does not currently expose item inventory or take/drop state; those phrases only narrate an interaction.
- **Intent type additions require `parish-core` wiring.** Adding a new `IntentKind` variant means adding a handler in `parish_core::game_session` — the two crates are coupled by the enum surface. Update both in the same commit.
- **Mention detection is for dialogue scoping.** `@name` syntax in user input is parsed by `mention.rs` and used to direct NPC responses. It does not affect movement or item interaction — only dialogue targeting.
- **Slash commands (`/save`, `/quit` etc.) are parsed in `parser.rs`** and bypass both intent paths entirely. They map directly to `Command` variants.

## Module map

`parser/` token classification + system commands, `commands.rs` command defs + handlers, `intent_types.rs` intent enums, `intent_local.rs` rule-based intent, `intent_llm.rs` LLM fallback, `mention.rs` @name dialogue scoping.
