# Judge verdict: BYOK Onboarding

## Scope assessment

Adds a parallel onboarding path to the existing Ollama-only flow. Tauri-only
in v1 — web and CLI shims register the new commands but return a
"desktop-only in v1" error. Per-category provider mixing is exposed through
the IPC payload (`category_overrides`) but not surfaced in the wizard UI in
v1; that's a v2 polish step.

Scope is bounded and intentional. The provider abstraction was already
comprehensive (`AnyClient` + `Provider` enum + `build_client` cover 15
providers); this PR fills the UX, persistence, and secret-storage gaps
around it.

## Code quality

- New backend-agnostic modules (`parish-core/src/secret_store.rs`,
  `parish-core/src/ipc/byok.rs`, `parish-config/src/user_config.rs`,
  `parish-inference/src/validate.rs`) are self-contained, well-documented,
  and have no runtime-crate dependencies.
- `keyring` is confined to `parish-tauri` and explicitly listed in
  `FORBIDDEN_FOR_BACKEND_AGNOSTIC` so a future leak fails the
  `backend_agnostic_crates_do_not_pull_runtime_deps` test mechanically.
- The onboarding gate in `parish-tauri/src/setup.rs` is small (one
  `needs_byok_onboarding` helper, one early-return branch), and it strictly
  preserves the existing Ollama bootstrap path when any explicit choice
  exists.
- The `fill_missing_models_from_presets` helper, which was already in
  `GameConfig`, is reused after `set_provider_config` so missing per-category
  models still get sensible defaults — no duplication.
- All BYOK IPC types live in `parish-core` per CLAUDE.md rule 12; the Tauri
  shims are thin (~10 LoC each).

## Security posture

- Keys never touch disk: `UserConfig` has no `api_key` field and a unit test
  guards against accidental future addition.
- `GetProviderConfigResult` exposes only `has_api_key`/`has_env_key` booleans;
  a unit test serializes the struct and asserts the raw key value never
  appears.
- Keys are trimmed before storage so clipboard whitespace can't make it into
  the keychain or HTTP `Authorization` header.
- One keychain record per provider (`provider:{name}`) so switching providers
  doesn't leak the previous key into the new context. The existing regression
  guard at `parish-config/src/provider.rs:1177-1193` carries forward because
  `GameConfig.api_key` is repopulated fresh on every `set_provider_config`.
- Resolution order documented:
  `CLI > PARISH_* env > standard provider env > keychain > parish.toml > defaults`.
  Keychain slots below standard env vars so power users aren't surprised.

## Test coverage

- 7 SecretStore tests (round-trip, missing, delete, isolation).
- 6 user_config tests (default-on-missing, round-trip, no-api-key invariant,
  onboarding marker, clear, env override).
- 12 validate() tests (every provider, retry-after parsing, 404→chat fallback,
  no-key short circuit, network error, 500 → Unexpected).
- 6 BYOK handler tests (happy path, MissingApiKey, MissingBaseUrl, key
  trimming, clear, get-doesn't-leak-key).
- 1 KeyringSecretStore round-trip (real OS keychain when available, skip
  otherwise).
- Architecture-fitness extended with `keyring` in
  `FORBIDDEN_FOR_BACKEND_AGNOSTIC`. 3 tests pass.
- 77 parish-tauri tests + 399 parish-core tests pass with no regression.

Total new tests: 32 unit + 1 architecture-fitness extension.

## Behavioral impact

- **Existing Ollama users**: zero change. `needs_byok_onboarding` returns
  false when any of (`.onboarded` sentinel, `PARISH_PROVIDER`, standard env
  key, non-default provider in resolved config) is set, so previous installs
  continue to flow through the existing bootstrap path.
- **New users without a GPU**: get a fork screen instead of a multi-GB Ollama
  download they can't afford.
- **Power users with `ANTHROPIC_API_KEY` exported**: still see the wizard but
  the key field is pre-populated indication-wise (the actual env-var value
  isn't surfaced; the field placeholder reads "(env var detected — leave
  blank to use it)") so a single click validates and saves.

## Resolved in this PR (during live `just run` iteration)

- Onboarding gate vs. event-listener race (overlay was mounting after the
  one-shot event fired) → fixed by also reading `needs_onboarding` from the
  snapshot (`fix(tauri,ui): persist needs_onboarding on setup snapshot`).
- MCP bridge wasn't spawned during onboarding because the bootstrap bailed
  early → fixed by reordering `mcp_bridge::spawn` before the gate
  (`fix(tauri): spawn MCP bridge before BYOK onboarding gate`).
- Env-var detection used the current provider, not the picked one → fixed
  with a new `list_byok_env_keys` IPC + frontend wiring
  (`fix(byok): honor env-var keys across the wizard + handler boundary`).
- Wizard hard-coded model defaults drifted from the backend presets → fixed
  by making `list_preset_models` the single source of truth
  (`fix(byok): single source of truth for default models`).
- Tauri capabilities: the four new commands are reachable from the WebView;
  verified by running the wizard end-to-end and observing
  `set_provider_config`, `validate_provider_config`, `list_byok_env_keys`,
  and `list_preset_models` all fire successfully against a live Anthropic
  key.
- A live `byok-flow.gif` runtime capture is still useful as marketing
  material but not load-bearing for correctness; the iterative `fix:`
  commits above are the transcript of the same walkthrough.

## Verdict

Verdict: sufficient

Technical debt: clear

Code is clean, test coverage is dense around the security-critical
invariants (no key on disk, no key in IPC return), the existing Ollama
path is preserved unchanged, and the live walkthrough has been driven
end-to-end with every observed issue resolved as a follow-up `fix:`
commit on this branch.
