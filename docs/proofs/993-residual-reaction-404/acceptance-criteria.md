# Acceptance Criteria: 993-residual-reaction-404

## Task

After PR #990's preset multi-slot fix, `just demo 2 5` still emits one
`HTTP 404 for url (http://localhost:8000/v1/chat/completions)` per run inside
`infer_player_message_reaction`. The reaction call lands on the dialogue slot
(`:8000`, 14B) instead of the reaction slot (`:8001`, 1.5B), then later calls
succeed.

Make `GameConfig::resolve_category_client` resolve the correct per-slot URL
regardless of whether `category_base_url` has been hydrated yet. Even if only
`category_model` is populated (the transient race window between
`fill_missing_models_from_presets` writes), the reaction client must be built
at the preset's `base_url` for that category, not the base provider's URL.

Subsequent reaction calls — and the first one — must hit the slot where the
model is loaded.

## Criteria

- **C1.** When `category_model[Reaction]` is set to the vllm-mlx 1.5B preset
  model but `category_base_url[Reaction]` is empty (the race state), a
  `resolve_category_client(Reaction, base=:8000)` call returns an OpenAI
  client whose configured URL is `http://localhost:8001`, not `:8000`.
  Observable via: unit test in `parish/crates/parish-core/src/ipc/config.rs`.
- **C2.** `has_override` treats a category-only model override as enough to
  build a fresh per-category client. A `GameConfig` with only
  `category_model[Reaction] = "1.5B"` resolves to a client distinct from the
  base client. Observable via: unit test.
- **C3.** When the provider's preset declares no per-category URL (single-slot
  providers like Ollama, Anthropic, OpenAI), `resolve_category_client` still
  falls back to the user's base URL — the new preset fallback is additive,
  not overriding. Observable via: unit test.
- **C4.** With the user's parish.toml from #993 (vllm-mlx base, only
  `[category_overrides.intent]` declared), running the full hydration sequence
  `apply_user_category_overrides → fill_missing_models_from_presets` followed
  by `resolve_category_client(Reaction, base=:8000)` yields a client at
  `http://localhost:8001` with model `Qwen2.5-1.5B-Instruct-4bit`.
  Observable via: unit test mirroring the user's parish.toml.
- **C5.** A headless CLI script run that issues player input does not emit
  `HTTP 404` lines for `infer_player_message_reaction` calls. Observable via:
  transcript captured from `cargo run -p parish -- --script ...`.

## Verification script

Run: `cargo run --manifest-path parish/Cargo.toml -p parish -- --script parish/testing/fixtures/play_993-residual-reaction-404.txt`

Expected signals in output:
- The script completes without `inference call failed in infer_player_message_reaction`.
- The script's `/preset vllm-mlx` followed by `/show.reaction.model` confirms
  the reaction model is the 1.5B preset.
- No `404` substring appears in the transcript.

Unit suite:
`cargo test -p parish-core resolve_category_client` and
`cargo test -p parish-core fill_missing_models` must pass, including new
regression tests `resolve_category_client_falls_back_to_preset_base_url`
and `resolve_category_client_model_only_override_triggers_per_category_client`.
