# BYOK Onboarding — Evidence

Adds an alternative to the Ollama download flow for users without a suitable
GPU. Equal-weight fork at the top of `SetupOverlay`; pick a hosted API
(Anthropic, OpenAI, OpenRouter, Groq, Google, xAI, opencode zen, Custom),
paste a key, validate live, save to OS keychain. Settings re-edit via
DebugInferenceTab modal.

Modeled after opencode (sst), Hermes, and openclaw onboarding flows.

## Scope (v1)

- **Tauri desktop only.** Web/CLI shims return "desktop-only in v1".
- Keys live in the OS keychain via the `keyring` crate.
- Non-secret choices persist to `~/Library/Application Support/Parish/parish.toml`
  (or XDG/AppData equivalents).
- 6 first-class provider cards plus an "Other…" expander; opencode zen rides
  through `Provider::Custom` with a labeled preset.

## Repository changes (10 commits, each green)

1. `refactor(core): SecretStore trait + InMemorySecretStore` — backend-agnostic
   abstraction at `parish-core/src/secret_store.rs`. 7 tests.
2. `feat(config): user_config_dir + UserConfig load/save` — TOML config persistence
   with no `api_key` field. 6 tests including
   `save_does_not_write_api_key_field` invariant guard.
3. `feat(inference): validate(provider, base_url, key) helper` — live ping with
   structured `ValidationOutcome`. 12 wiremock tests covering every provider +
   the 404→chat-completions fallback for older self-hosted servers.
4. `feat(tauri): KeyringSecretStore + arch-fitness guard` — `keyring` v3 added
   to parish-tauri, listed in `FORBIDDEN_FOR_BACKEND_AGNOSTIC` so it can never
   leak into a leaf crate.
5. `feat(core): BYOK IPC handlers (set/get/clear/validate provider config)` —
   shared backend-agnostic handlers in `parish-core/src/ipc/byok.rs`. 6 unit
   tests covering happy path, MissingApiKey rejection, MissingBaseUrl rejection
   (Custom), key trimming, clear round-trip, key never returned by getter.
6. `feat(tauri): BYOK IPC commands + onboarding gate` — four `#[tauri::command]`
   shims; `bootstrap_inference_provider` checks `needs_byok_onboarding` and
   emits `EVENT_SETUP_NEEDS_ONBOARDING` instead of running Ollama bootstrap on
   first launch.
7. `feat(ui): BYOK fork + onboarding wizard` — `byokProviders.ts`,
   `ByokFork.svelte`, `ByokOnboarding.svelte`, `ipc.ts` extensions; SetupOverlay
   forks on `needsOnboarding`.
8. (rolled into 7)
9. `feat(ui): DebugInferenceTab reopens BYOK in modal` — single button that
   re-mounts the wizard with `mode="modal"`.
10. (this bundle)

## Test gates

```sh
cargo test -p parish-core --lib secret_store              # 7  passed
cargo test -p parish-config --lib user_config             # 6  passed
cargo test -p parish-inference --lib validate             # 12 passed
cargo test -p parish-tauri --lib keychain                 # 1  passed (real keychain)
cargo test -p parish-core --lib ipc::byok                 # 6  passed
cargo test -p parish-core --test architecture_fitness     # 3  passed (incl. keyring guard)
cargo test -p parish-core --test wiring_parity            # 6  passed
cargo test -p parish-core                                 # 399 passed, 5 ignored
cargo test -p parish-tauri                                # 77  passed
cargo build --workspace                                   # green
```

## Resolution order documented

`CLI flag > PARISH_* env > standard provider env (ANTHROPIC_API_KEY etc.)
 > OS keychain > parish.toml > defaults`

The keychain ranks **below** standard provider env vars so power users with
`ANTHROPIC_API_KEY` exported in their shell aren't surprised — the wizard
detects the env var (`get_provider_config.has_env_key`) and pre-fills the
field instead.

## Invariants enforced by tests

- `UserConfig` never serializes an `api_key` field
  (`save_does_not_write_api_key_field`).
- `GetProviderConfigResult` never carries the raw key
  (`get_provider_config_does_not_return_key`).
- `SetProviderConfig` rejects cloud providers without a key
  (`set_provider_config_rejects_cloud_without_key`).
- `Provider::Custom` requires an explicit base URL
  (`set_provider_config_custom_requires_base_url`).
- API keys are trimmed before storage
  (`set_provider_config_trims_key_whitespace`).
- `keyring` cannot be added to any backend-agnostic crate
  (architecture-fitness test).

## Manual verification (still required before merge)

The runtime walkthrough below must be captured as `byok-flow.gif` and
re-uploaded to this directory before merge. Mechanically it cannot be
captured in CI; it requires an interactive Tauri session.

1. Wipe `~/Library/Application Support/Parish/.onboarded` and `parish.toml`.
2. `unset ANTHROPIC_API_KEY OPENAI_API_KEY OPENROUTER_API_KEY` in the launch shell.
3. `just run` — expect the BYOK fork screen rather than the Ollama spinner.
4. Click "Use a hosted API (BYOK)" → Anthropic → paste a real key.
5. Validation succeeds; save & continue.
6. First NPC dialogue streams via Anthropic (debug panel shows
   `provider: anthropic`).
7. Quit and relaunch — game starts directly without the wizard.
8. Open DebugInferenceTab → "Change provider or key…" → switch to OpenRouter
   with a different key. Next NPC reply streams via OpenRouter.
