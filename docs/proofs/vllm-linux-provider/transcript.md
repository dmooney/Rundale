Evidence type: gameplay transcript

# vllm Linux/Windows Provider — Proof Transcript

## Feature

Adds a `vllm` provider mod (Linux/Windows, CUDA/ROCm) alongside the existing
`vllmmlx` mod (macOS Apple Silicon). Ported to the data-driven provider-mod
system (PR #968): `providers/vllm.toml` is embedded at compile time and loaded
into `ProviderRegistry` at startup. Mirrors the vllm-mlx two-slot loadout in
`parish-inference/src/setup.rs`: `VllmSlot`, `VllmProcess` (probe-then-spawn,
60s readiness wait), `RuntimeProcesses.vllm` field, and
`GameConfig::vllm_extra_slots()`. Wired through CLI, Tauri, and web-server
entry points (mode parity).

## Test run: `cargo test -p parish-config` (vllm-scoped)

```
$ target/debug/deps/parish_config-737e6240878e2631 vllm
running 6 tests
test provider::tests::test_vllm_provider_defaults ... ok
test provider::tests::recommended_for_platform_picks_vllm_mlx_on_macos_else_vllm ... ok
test provider::tests::test_vllm_provider_from_str ... ok
test provider::tests::vllm_mlx_aliases_resolve ... ok
test provider::tests::test_resolve_config_vllm_custom_base_url ... ok
test provider::tests::test_resolve_config_vllm ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 119 filtered out
```

Asserts:

- `"vllm"` and `"VLLM"` strings resolve to provider id `vllm` (Linux variant),
  not `vllmmlx`.
- `Provider::recommended_for_platform()` returns id `vllm` on non-macOS hosts.
- vllm-mlx aliases (`vllm-mlx`, `vllm_mlx`, `vllmmlx`, `VLLM-MLX`) still
  resolve to `vllmmlx` — the new mod did not steal them.
- `resolve_config(provider="vllm", model="Qwen/Qwen2.5-14B-Instruct")` yields
  `id=vllm`, `base_url=http://localhost:8000`, no API key, expected model.
- Custom `base_url` override propagates.

## Test run: registry coverage

```
$ target/debug/deps/parish_config-737e6240878e2631 registry
running 4 tests
test provider::tests::test_registry_has_all_providers ... ok
test provider::tests::registry_all_returns_sorted_list_of_all_providers ... ok
test provider::tests::registry_featured_returns_subset_of_all ... ok
test provider::tests::registry_lookup_finds_by_id_and_rejects_unknown ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured
```

Registry now embeds 23 mods (22 from #968 + the new `vllm`).
`registry_all_returns_sorted_list_of_all_providers` enforces `all.len() >= 22`.

## Test run: full targeted suite

```
$ cargo test -p parish-config -p parish-inference -p parish-core
cargo test: 809 passed, 11 ignored (15 suites)
```

Zero regressions across the three crates touched.

## Quality gate

```
cargo fmt --all --check         → clean
cargo clippy --workspace --all-targets -- -D warnings → clean
cargo check --workspace         → clean
```

## Provider mod (`parish-config/providers/vllm.toml`)

```
id = "vllm"
display_name = "vLLM (CUDA/ROCm)"
aliases = []
kind = "local"
default_base_url = "http://localhost:8000"
requires_api_key = false
requires_model = true
blurb = "Self-hosted OpenAI-compatible server for Linux/Windows (CUDA/ROCm)."
signup_url = "https://docs.vllm.ai"
featured = false

[[presets]]
key = "recommended"
label = "Recommended"
dialogue = "Qwen/Qwen2.5-14B-Instruct"
simulation = "Qwen/Qwen2.5-1.5B-Instruct"
intent = "Qwen/Qwen2.5-1.5B-Instruct"
reaction = "Qwen/Qwen2.5-1.5B-Instruct"
```

The colliding `"vllm"` alias on `vllm_mlx.toml` is dropped so id-lookup wins.

## Mode parity verification

`setup_provider_client` signature now takes `extra_vllm_mlx_slots` AND
`extra_vllm_slots`. All three entry points pass both:

- `parish-cli/src/main.rs:295` — passes `&[], &[]` (headless runs no extras).
- `parish-server/src/lib.rs:711` — calls `config.vllm_mlx_extra_slots()` and
  `config.vllm_extra_slots()`.
- `parish-tauri/src/setup.rs:269` — destructures both extras from
  `GameConfig`.

Architecture-fitness tests (CLAUDE.md rule #1, enforced) pass — no leaf-crate
duplication, no backend-leakage into shared logic.

## Behavioral invariants preserved

- `vllm-mlx`, `vllm_mlx`, `vllmmlx`, `VLLM-MLX` still resolve to `vllmmlx`.
- macOS `recommended_for_platform()` still picks `vllmmlx` (≥16 GB unified
  memory) or `simulator` (<16 GB).
- All `vllmmlx` setup paths (slot dedup, spawn, env wiring) unchanged.
- `RuntimeProcesses::stop()` and `Drop` now stop `vllm` slots in addition to
  `ollama`/`vllm_mlx`; no leak.
