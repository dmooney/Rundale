Evidence type: gameplay transcript
Date: 2026-05-16
Branch: claude/evaluate-github-models-ooUj2

## Requirement

Add GitHub Models as a first-class inference provider so CI can run real-model
tests at no cost, using the auto-injected `GITHUB_TOKEN` with
`permissions: models: read`.

## Changes

**`parish-config`:**
- `providers/github_models.toml` — new provider file: id `github_models`,
  kind `openai-compat`, default base URL `https://models.github.ai/inference`,
  `api_key_env_var = GITHUB_TOKEN`, preset with Llama-3.1-405B (dialogue),
  Llama-3.1-70B (simulation/reaction), Phi-4 (intent).
- `src/provider.rs` — `Provider::github_models()` named constructor added,
  consistent with all other cloud providers.

**`parish-inference`:**
- `src/openai_client.rs` — added `completions_path` field (default
  `/v1/chat/completions`); `with_completions_path()` builder lets
  GitHub Models use `/chat/completions` (no `/v1/` prefix).
- `src/lib.rs` — `build_client()` dispatches `provider.id() == "github_models"`
  to an `OpenAiClient` with `.with_completions_path("/chat/completions")`.
- `src/validate.rs` — `probe_github_models()` probes `GET /models` first
  (no specific model needed, avoids org-allowlist issues); falls back to
  `POST /chat/completions` on 404. Dispatch via `_ if provider.id() == "github_models"`.

**Eval harness (`parish/testing/eval/`):**
- `player_agent.py` — drives parish-server as a human-like player; reads
  world state, calls GitHub Models API for commands.
- `judge.py` — calls gpt-4o via GitHub Models to score session logs.
- 6 scenario scripts + 6 judge rubrics covering smoke, intent, reactions,
  tier2 simulation, dialogue, and full session.

**CI (`.github/workflows/eval-inference.yml`):**
- Nightly + `workflow_dispatch` eval pipeline; no stored secrets needed.

## Test results

Command:

```sh
cargo test -p parish-config -p parish-inference
```

Result:

```
parish-config: 129 passed, 0 failed
parish-inference (lib): 28 passed, 0 failed  (includes 16 validate tests)
parish-inference github_models: 3 passed, 0 failed
  - github_models_validate_probes_models_endpoint_not_v1
  - github_models_validate_falls_back_to_chat_when_models_404
  - github_models_validate_maps_401_to_auth_failed
```

Full workspace: all test suites pass, 0 failures.

## Format and clippy

```
cargo fmt --check  → clean
cargo clippy -- -D warnings  → clean
```

## Architecture fitness

GitHub Models uses `ProviderKind::OpenAiCompat` (via `openai-compat` in TOML),
dispatched through the existing `OpenAiClient`. The only provider-specific logic
is the `completions_path` override and the custom validation probe — both
isolated inside `parish-inference`. No backend-agnostic crates depend on
axum/tower/tauri. Architecture fitness tests pass.

## Rate limit strategy

Player (`microsoft/Phi-4`): 150 RPD free quota.
NPC/in-game (`meta/Llama-3.1-70B-Instruct`): separate 150 RPD quota.
Judge (`openai/gpt-4o`): 50 RPD; 6 calls per nightly run.
