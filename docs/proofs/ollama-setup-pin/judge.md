Verdict: sufficient
Technical debt: clear

The change resolves the loading/setup disconnect reported by the user:
auto-setup downloaded one Ollama model that did not match the model the
presets configured, so every inference category requested a model that
was never pulled.

All five acceptance criteria from `evidence.md` are met:

1. `GameConfig::auto_setup_model` field and `pin_setup_model(model)`
   helper land in `parish-core/src/ipc/config.rs`. Helper writes the
   provided model into `model_name` and every `category_model[cat]` slot
   in a single call, and records the model in `auto_setup_model` so
   `/preset ollama` can re-pin without re-running auto-setup.
2. Tauri (`parish-tauri/src/lib.rs`) and web (`parish-server/src/lib.rs`)
   bootstrap branch on `matches!(provider.., Provider::Ollama)`. Cloud
   bootstrap still calls `fill_missing_models_from_presets()` so
   Anthropic / OpenAI / etc. retain their per-role tier mapping.
3. CLI's `resolve_category_configs` (`parish-cli/src/config.rs:276`)
   gates the preset-fill `or_else` on `provider != Provider::Ollama`.
   `build_inference_clients` (`parish-cli/src/main.rs:333`) gates its
   preset-fill loop the same way. Per-category overrides at higher
   layers (TOML, env, CLI) are still honoured.
4. `Command::ApplyPreset(Ollama)` (`parish-core/src/ipc/commands.rs:467`)
   re-pins `auto_setup_model` when `Some`, falling back to today's
   static-preset write only when no auto-setup model is recorded.
5. `Provider::Ollama::preset_models()` is unchanged in
   `parish-config/src/presets.rs`; `local_providers_have_complete_presets`
   still passes.

Test suite: 2321 passed, 17 ignored across 53 suites. Clippy clean with
`-D warnings`. Eight new unit tests lock in the invariants
(`pin_setup_model_writes_all_four_category_slots`,
`apply_preset_ollama_uses_auto_setup_model_when_present`,
`resolve_category_configs_ollama_override_without_model_skips_preset_fill`,
and five sibling tests covering overwrite, no-op fill, runtime
resolution, fallback, and the cloud-preset regression guard).

Mode parity (CLAUDE.md rule #2) is honoured — Tauri, web, and CLI all
apply the same single-pulled-model story for Ollama. The architecture
fitness test in `parish-core/tests/architecture_fitness.rs` is unaffected
by this change.

Live verification of the Tauri GUI flow requires a host with a GPU and
Ollama installed; the unit-test coverage above replaces that gap for CI
purposes. A reviewer with the right hardware should confirm `/preset
ollama` preserves the gemma4 tag and all four inference categories hit
Ollama with the same model name in `OLLAMA_DEBUG=1` logs.

No placeholder debt markers, no panicking stubs, no carve-out comments.
