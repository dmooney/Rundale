Evidence type: gameplay transcript

# Provider Mod System — Proof Transcript

## Feature
Replace the hardcoded 15-variant `Provider` enum with a data-driven TOML
discovery system. All 22 provider mods (15 migrated + 7 new) are embedded at
compile time via `build.rs` and loaded into a `ProviderRegistry` at startup.

## Test run: `cargo test -p parish-config --lib`

All 106 tests in `parish-config` pass, including `test_registry_has_all_providers`
which asserts every expected provider ID is present in the registry:

```
test provider::tests::test_registry_has_all_providers ... ok
test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Providers verified by the test (15 original + 7 new = 22 total):

| Provider ID     | Kind          | New? |
|-----------------|---------------|------|
| anthropic       | anthropic     |      |
| openai          | openai-compat |      |
| openrouter      | openai-compat |      |
| google          | openai-compat |      |
| groq            | openai-compat |      |
| xai             | openai-compat |      |
| mistral         | openai-compat |      |
| deepseek        | openai-compat |      |
| together        | openai-compat |      |
| nvidia-nim      | openai-compat |      |
| ollama          | local         |      |
| lmstudio        | local         |      |
| vllmmlx         | local         |      |
| custom          | openai-compat |      |
| simulator       | simulator     |      |
| vercel-ai       | openai-compat | ✓    |
| qwen            | openai-compat | ✓    |
| zhipu           | openai-compat | ✓    |
| moonshot        | openai-compat | ✓    |
| siliconflow     | openai-compat | ✓    |
| cohere          | openai-compat | ✓    |
| scaleway        | openai-compat | ✓    |

## Test run: full workspace

```
test result: ok. 152 passed; 0 failed; 0 ignored; 0 measured
```

## Quality gate

```
cargo fmt --check  → clean
cargo clippy -- -D warnings  → clean
cargo test --workspace --lib  → 152/152 passed
```

## Binary smoke test

```
cargo run --bin parish -- --provider simulator --script /dev/null
```

Exits cleanly (code 0) — the registry loaded all 22 mods without panic.

## BYOK IPC changes

- `list_preset_models` now returns `BTreeMap<String, Vec<ProviderPresetOption>>`
  where each entry is an array of named presets (key + label + per-category model).
- `list_byok_env_keys` now iterates `registry().all()` to include the 7 new providers.
- `opencode zen` removed from the BYOK wizard; Vercel AI Gateway added as a
  proper provider with `needs_base_url_from_user = true`.

## Behavioral invariants preserved

- Providers that previously required an API key still do.
- Providers that previously did not require a base URL still don't.
- All named constructors (`Provider::openrouter()`, etc.) still work and are
  checked by the existing 106 parish-config unit tests.
- `Provider::from_str_loose` still resolves all known aliases
  (test_provider_from_str_loose passes).
