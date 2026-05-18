# Judge: 993-residual-reaction-404

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Per-criterion verification

- **C1** — Race-state routing puts 1.5B reaction on `:8001`:
  Verified by unit test `resolve_category_client_falls_back_to_preset_base_url`
  in `parish/crates/parish-core/src/ipc/config.rs`. Asserts
  `openai.base_url() == "http://localhost:8001"` when `category_provider[Intent]`
  is set but `category_base_url` is empty — the exact race window the issue
  hypothesises. `cargo test … -- resolve_category_client_falls_back_to_preset_base_url` → `1 passed`.

- **C2** — Model-only override builds a fresh per-category client:
  Verified by `resolve_category_client_model_only_override_triggers_per_category_client`.
  With only `category_model[Reaction] = "1.5B"` set, the resolver returns a
  client at `:8001` (preset URL), not the base `:8000` client. The previous
  `has_override` did not include `category_model.contains_key(&cat)` — that
  was the original silent miss-route. `1 passed`.

- **C3** — Single-slot providers unchanged:
  Verified by `resolve_category_client_preset_fallback_is_inert_for_single_slot_provider`.
  An Ollama config with a reaction-model override still resolves to
  `http://localhost:11434` (the user's base URL) because `ollama.toml` ships
  no `[presets.base_urls]`. `1 passed`.

- **C4** — Issue #993 user config end-to-end:
  Verified by `issue_993_user_config_hydration_routes_reaction_to_slot_8001`.
  Replays the user's actual parish.toml (vllm-mlx base + intent-only
  category override) through `apply_user_category_overrides` +
  `fill_missing_models_from_presets`, then confirms
  `resolve_category_client(Reaction, base=:8000)` produces an OpenAI client
  at `http://localhost:8001` with model `mlx-community/Qwen2.5-1.5B-Instruct-4bit`.
  `1 passed`.

- **C5** — No 404 in CLI transcript:
  Verified by `docs/proofs/993-residual-reaction-404/transcript.txt`. The
  fixture runs `/preset vllm-mlx`, inspects per-category routing, takes a
  `say` turn that fires the reaction code path, and re-inspects routing.
  `grep -c "404\|inference call failed" transcript.txt` → `0`.
  Reaction-emoji event present (`Peig Hannigan 😊`) confirming the reaction
  path executed end-to-end without falling through to the 404 logging branch.

## Workspace integrity

`cargo test --manifest-path parish/Cargo.toml --workspace --lib` →
`2258 passed, 7 ignored`. No prior tests regressed. The behaviour change
in `fill_missing_models_from_presets` (dropping the `filled_model` gate on
URL fill) is covered by existing tests that exercise model + URL fill via
the canonical-base-URL guard.

## Technical-debt review

The fix is two-fold and additive:

1. `has_override` now includes `category_model.contains_key(&cat)`. This
   tightens a check that was already documented to "build a per-category
   client if the provider, URL, or key is overridden" — `model` was missing
   from the list, which was a latent bug, not new debt.
2. `resolve_category_client` URL chain prefers `provider.preset_base_url(cat)`
   over `self.base_url` when the per-category URL is empty. For
   single-slot providers `preset_base_url` is `None`, so the chain is
   inert — covered by C3.

No new unwraps, no new feature flags, no new dependencies, no skipped tests.
