# limerick-input — agent scope

Player input parsing and command interpretation for the Limerick engine. Backend-agnostic leaf crate — consumed by every entry point via `limerick-core`. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo test -p limerick-input                       # unit tests
cargo test -p limerick-input --test '*'            # integration (llm fallback)
```

## Local gotchas

- **Leaf-crate dependency rule.** Depends only on `limerick-types`, `limerick-config`, `limerick-inference`. Never take a dependency on `limerick-core` or any runtime crate (tauri, axum, engine).
- **Intent semantics live in `intent_contract.rs`, not in a runtime.** The prompt, the response payload shape, and the post-response validation are shared: `intent_llm.rs` is the desktop provider transport and `limerick_core::mobile` is the Limerick Endpoints transport. Adding a rule to one runtime instead of here re-creates the divergence that lost mobile intent inference entirely (#1993). The published `mobile/endpoint/rundale-intent-v1.json` embeds `INTENT_SYSTEM_PROMPT` verbatim and a `limerick-core` contract test fails on drift.
- **LLM is the fallback, not the primary path.** `intent_local.rs` + `parser.rs` handle all known command forms (movement, look/examine, and physical interaction narration) first. `intent_llm.rs` is only invoked for unrecognised input — keep local coverage broad to avoid unnecessary inference calls. Limerick does not currently expose item inventory or take/drop state; those phrases only narrate an interaction.
- **Intent type additions require `limerick-core` wiring.** Adding a new `IntentKind` variant means adding a handler in `limerick_core::game_session` — the two crates are coupled by the enum surface. Update both in the same commit.
- **Mention detection is for dialogue scoping.** `@name` syntax in user input is parsed by `mention.rs` and used to direct NPC responses. It does not affect movement or item interaction — only dialogue targeting.
- **Slash commands (`/save`, `/quit` etc.) are parsed in `parser.rs`** and bypass both intent paths entirely. They map directly to `Command` variants.

## Module map

`parser/` token classification + system commands, `commands.rs` command defs + handlers, `intent_types.rs` intent enums, `intent_local.rs` rule-based intent, `intent_contract.rs` the portable prompt/payload/validation shared by every runtime, `intent_llm.rs` the desktop LLM transport over it, `mention.rs` @name dialogue scoping.
