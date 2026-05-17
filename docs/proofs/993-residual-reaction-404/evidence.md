Evidence type: live gameplay transcript

# Evidence: 993-residual-reaction-404

Live headless CLI transcript captured from:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish -- \
  --script parish/testing/fixtures/play_993-residual-reaction-404.txt
```

(`parish-cli` is the crate name; `parish` is the binary name — accepted live
signal under rule #10 live-proof tier.)

Transcript: [transcript.txt](transcript.txt).

## Criterion → evidence map

### C1. Race-state resolve_category_client routes 1.5B reaction to :8001

Unit-test proof, not in the transcript itself.
`parish/crates/parish-core/src/ipc/config.rs::tests::resolve_category_client_falls_back_to_preset_base_url`
asserts `openai.base_url() == "http://localhost:8001"` when
`category_provider[Intent] = "vllmmlx"` and `category_base_url` is empty.

```sh
cargo test --manifest-path parish/Cargo.toml -p parish-core --lib \
  resolve_category_client_falls_back_to_preset_base_url
```

Result: `1 passed`.

### C2. Model-only override triggers per-category client

Unit-test proof.
`parish/crates/parish-core/src/ipc/config.rs::tests::resolve_category_client_model_only_override_triggers_per_category_client`
asserts that setting only `category_model[Reaction] = 1.5B` on a vllm-mlx
config builds a fresh OpenAI client at `:8001` (not the base `:8000`).

Result: `1 passed`.

### C3. Single-slot provider behaviour unchanged

`parish/crates/parish-core/src/ipc/config.rs::tests::resolve_category_client_preset_fallback_is_inert_for_single_slot_provider`
asserts an Ollama config with a model-only reaction override still resolves
to `http://localhost:11434` (the user's base) because Ollama's preset has no
`[presets.base_urls]`.

Result: `1 passed`.

### C4. User parish.toml from #993 routes reaction to :8001

`parish/crates/parish-core/src/ipc/config.rs::tests::issue_993_user_config_hydration_routes_reaction_to_slot_8001`
applies an exact replica of the user's parish.toml category overrides
(intent only, with provider/model/url), runs
`apply_user_category_overrides → fill_missing_models_from_presets`, then
verifies `resolve_category_client(Reaction)` builds at `:8001` with the
1.5B model.

Result: `1 passed`.

### C5. Headless CLI transcript: no 404s

Transcript lines (verbatim, no 404):

> Line 1: `{"command":"/preset vllm-mlx", … "response":"Applied vllmmlx preset (Dialogue/Simulation/Intent/Reaction)." …}`
> Line 6: `{"command":"/model.intent", … "response":"intent model: mlx-community/Qwen2.5-1.5B-Instruct-4bit" …}`
> Line 7: `{"command":"/model.reaction", … "response":"reaction model: mlx-community/Qwen2.5-1.5B-Instruct-4bit" …}`
> Line 13: `{"command":"say Hello, friend.", "result":"npc_not_available", … "new_log_lines":["Peig Hannigan 😊"]}`

A rule-based or LLM reaction (`Peig Hannigan 😊`) fired without producing
any `inference call failed in infer_player_message_reaction` log line.

`grep -c "404\|inference call failed" transcript.txt` → `0`.

## Workspace test sweep

```sh
cargo test --manifest-path parish/Cargo.toml --workspace --lib
```

Result: `2258 passed, 7 ignored`. No regressions across the workspace.

## Format + lint

- `cargo fmt --manifest-path parish/Cargo.toml --all --check` → clean
- `cargo clippy --manifest-path parish/Cargo.toml -p parish-core --no-deps -- -D warnings` → no issues
