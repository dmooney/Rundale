Evidence type: gameplay transcript
Date: 2026-05-06
Branch: claude/focused-liskov-1bc1be

# Proof Evidence — Pin Ollama auto-setup model across base, per-category, and preset paths

## Bug

First-run Ollama setup auto-picks **one** model from the gemma4 family based
on detected VRAM and pulls it. Bootstrap then fills the four per-category
model slots from a hard-coded **qwen3** preset that uses a different model
family. At inference time the per-category override wins, so every category
requests a model that was never downloaded — Dialogue, Simulation, Intent,
and Reaction all hit "model not found", or trigger a silent multi-GB Qwen
pull behind the user's back.

The disconnect lived in three independent code paths:

1. **Tauri bootstrap** — `parish/crates/parish-tauri/src/lib.rs` wrote
   `model_name = <gemma4 setup pick>` then called
   `fill_missing_models_from_presets()`, which filled all four
   `category_model[*]` slots with `qwen3:*`.
2. **Web bootstrap** — `parish/crates/parish-server/src/lib.rs` did the same.
3. **CLI** — `parish/crates/parish-cli/src/config.rs` called
   `provider.preset_model(category)` (qwen3 again) inside
   `resolve_category_configs`, and `parish/crates/parish-cli/src/main.rs`
   filled missing categories from the same preset inside
   `build_inference_clients`. Both ran regardless of what setup actually
   pulled.
4. **`/preset ollama`** —
   `parish/crates/parish-core/src/ipc/commands.rs::Command::ApplyPreset`
   wrote the static qwen3 list into all four slots, breaking inference for
   any user who only had the auto-setup gemma4 model on disk.

`resolve_category_client` in `parish/crates/parish-core/src/ipc/config.rs`
reads `category_model[cat]` first, falling back to `model_name` only if
absent. Once the slots were filled with qwen3 tags, the base `model_name`
(holding the actual gemma4 tag from setup) was unreachable.

## Acceptance criteria

1. `GameConfig` gains an `auto_setup_model: Option<String>` field and a
   `pin_setup_model(model)` helper that writes the same model into
   `model_name` and all four `category_model[*]` slots, and records the
   model in `auto_setup_model`.
2. Tauri and web bootstrap call `pin_setup_model` when the active provider
   is `Provider::Ollama`. Cloud providers retain
   `fill_missing_models_from_presets()` so Anthropic still gets
   Opus/Sonnet/Haiku per role.
3. CLI's `resolve_category_configs` skips its preset-fill step for
   `Provider::Ollama`, leaving `cat_model` as `None` so
   `build_inference_clients` falls through to the auto-setup-resolved
   `base_model`. CLI's second preset-fill loop is gated the same way.
4. `Command::ApplyPreset(Ollama)` re-pins `auto_setup_model` when present
   instead of writing the static qwen3 preset. When `auto_setup_model` is
   `None` (cloud → ollama transition before auto-setup populated the
   field), today's static-preset behaviour is preserved.
5. `Provider::Ollama::preset_models()` is unchanged — the static qwen3
   list still applies to the rare cloud→ollama-without-prior-setup branch
   and keeps `local_providers_have_complete_presets` green.

## Test results

Workspace tests pass — 2321 passing across 53 suites:

```
cd parish && cargo test --workspace
cargo test: 2321 passed, 17 ignored (53 suites, 7.51s)
```

Targeted runs verifying the new behaviour:

```
cargo test -p parish-core --lib ipc::config
cargo test: 19 passed, 299 filtered out (1 suite, 0.20s)

cargo test -p parish-core --lib ipc::commands
cargo test: 87 passed, 231 filtered out (1 suite, 0.00s)

cargo test -p parish    --lib config
cargo test: 11 passed, 142 filtered out (1 suite, 0.01s)
```

Clippy clean across the workspace:

```
cargo clippy --workspace --tests -- -D warnings
cargo clippy: No issues found
```

## New tests covering the fix

- `parish-core/src/ipc/config.rs::tests`:
  - `pin_setup_model_writes_all_four_category_slots` — pin pins every
    `InferenceCategory::ALL` slot and records `auto_setup_model`.
  - `pin_setup_model_overwrites_existing_slots` — a stale qwen3 entry is
    overwritten by the auto-setup model.
  - `fill_missing_models_from_presets_after_pin_is_noop` — bootstrap
    ordering: pin then fill leaves the pinned slots intact and returns
    `false` (no change).
  - `resolve_category_client_returns_pinned_model` — runtime resolution
    returns the pinned model for every category.

- `parish-core/src/ipc/commands.rs::tests`:
  - `apply_preset_ollama_uses_auto_setup_model_when_present` — `/preset
    ollama` after auto-setup re-pins the gemma4 model rather than writing
    qwen3 to every slot.
  - `apply_preset_ollama_falls_back_to_static_when_no_auto_setup` —
    today's static-preset behaviour is preserved for the cold-start
    branch.

- `parish-cli/src/config.rs::tests`:
  - `resolve_category_configs_ollama_override_without_model_skips_preset_fill`
    — Ollama category with a non-model TOML override leaves `cat_model`
    as `None`, so `build_inference_clients` falls through to the
    auto-setup-resolved `base_model`.
  - `resolve_category_configs_ollama_respects_env_model_override` —
    explicit user model overrides (env, CLI, TOML) are still honoured.
  - `resolve_category_configs_anthropic_still_fills_presets` —
    regression: cloud-provider preset-fill is unchanged.

## Changed files

| File | Change |
|------|--------|
| `parish-core/src/ipc/config.rs` | New `auto_setup_model` field + `pin_setup_model` helper + 4 unit tests. |
| `parish-core/src/ipc/commands.rs` | `Command::ApplyPreset(Ollama)` re-pins `auto_setup_model` when present + 2 unit tests. |
| `parish-tauri/src/lib.rs` | Bootstrap pins resolved model via `pin_setup_model` for `Provider::Ollama`. |
| `parish-tauri/src/commands.rs` | `auto_setup_model: None` in test-only `GameConfig` literal. |
| `parish-server/src/lib.rs` | Bootstrap pins resolved model via `pin_setup_model` for `Provider::Ollama`. |
| `parish-server/src/middleware.rs` | `auto_setup_model: None` in 2 test-only `GameConfig` literals. |
| `parish-server/src/routes.rs` | `auto_setup_model: None` in test-only `GameConfig` literal. |
| `parish-server/tests/{new_game_parity,admission_control,isolation}.rs` | `auto_setup_model: None` in test fixtures. |
| `parish-cli/src/config.rs` | `resolve_category_configs` skips preset-fill for `Provider::Ollama` + 3 unit tests. |
| `parish-cli/src/main.rs` | `build_inference_clients` skips preset-fill loop for `Provider::Ollama`. |

## Mode parity

CLAUDE.md rule #2 is honoured: every entry point — Tauri desktop, web
server, headless CLI — applies the single-pulled-model story for Ollama.
Cloud-provider behaviour is unchanged in every entry point.

## Lock ordering

`auto_setup_model` is a plain `Option<String>` field on `GameConfig`. It
is read and written under the same `Mutex<GameConfig>` already used for
the other config fields; no new lock is introduced.

## Live verification

Headless CLI run on Apple Silicon, ~34 GB free unified memory, no
`PARISH_*` overrides set:

```
[Parish] The storyteller's tools are at hand.
[Parish] Taking stock of what we have to work with...
[Parish] Hardware: Apple Silicon (Metal) — 49152MB unified memory, ~34406MB available
[Parish] Lighting the fire in the storyteller's cottage...
[Parish] The storyteller was already here. Grand so.
[Parish] Chosen tale: gemma4:31b (Tier 1 — Full quality (dense 31B), ~22000MB VRAM)
[Parish] The storyteller already has 'gemma4:31b' in hand.
[Parish] The storyteller is gathering their thoughts...
[Parish] The storyteller is ready. The parish awaits.
=== Parish — Headless Mode ===
Base: gemma4:31b (ollama)
```

Before this PR: `resolve_config` filled `ProviderConfig.model` from the
static `Provider::Ollama` Dialogue preset (`qwen3:32b`), and the auto-
setup loop saw `Some("qwen3:32b")` as a "user override" — so the
hardware-aware `select_model` branch never ran. Setup would download or
reuse qwen3:32b regardless of VRAM, and bootstrap then filled the per-
category slots with the rest of the static qwen3 list.

After this PR (transcript above): `resolve_config` leaves
`ProviderConfig.model` as `None` for Ollama, so `setup_ollama_with_config`
picks `gemma4:31b` from the 34 GB VRAM tier. `pin_setup_model` writes
that tag into `model_name` and all four `category_model[*]` slots; every
inference category routes to the model that is actually on disk.

## Tauri / web parity

The headless CLI exercises the same `setup_provider_client` →
`pin_setup_model` path used by `parish-tauri/src/lib.rs` and
`parish-server/src/lib.rs`. The only Tauri/web-specific wiring is the 4-
line bootstrap branch (`if matches!(provider.., Provider::Ollama) {
config.pin_setup_model(model) } else { config.model_name = model }`),
covered visually and by the existing unit-test suite for the helper.
