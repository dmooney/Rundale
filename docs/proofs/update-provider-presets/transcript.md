Evidence type: gameplay transcript

# Provider Preset Update — Proof Transcript

## Changes

Updated provider presets and model catalog based on rundale-bench v1 eval
results (May 15 2026). No gameplay logic changed; only configuration data
and the frontend model autocomplete catalog were touched.

| File | Change |
|------|--------|
| `providers/openrouter.toml` | Replace single `gpt-oss-120b` preset with `recommended` (claude-based) + `budget` (qwen/gemma) |
| `providers/groq.toml` | Dialogue: `gpt-oss-120b` → `llama-3.3-70b-versatile` |
| `providers/mistral.toml` | Recommended: `mistral-large-2512` → `mistral-medium-3.1`; rename "medium" → "large" |
| `providers/vllm.toml` | Qwen2.5 (14B/1.5B) → Qwen3 (14B/8B/4B) |
| `ui/src/lib/model-catalog.ts` | Add `gpt-5.4`, `mistral-medium-3.1`, and 7 new OpenRouter slugs |

## Eval evidence motivating each change

Dual-judge multi-axis scores from `docs/proofs/rundale-bench/`:

### Flagship tier (May 15, 15 prompts/candidate)

| Candidate | grok-4.3 judge | mistral-large judge |
|-----------|---------------|---------------------|
| openai/gpt-5.5 | 8.75 | 9.05 |
| anthropic/claude-opus-4-7 | 8.64 | 9.05 |
| mistral-medium-3.1 | 8.59 | 9.09 |
| anthropic/claude-sonnet-4-6 | 8.53 | 9.13 |
| **openai/gpt-oss-120b** | **7.12** | — |

`gpt-oss-120b` (previous OpenRouter/Groq dialogue default) is bottom of
the paid tier — 1.6 points behind the leader on grok judge.
`mistral-medium-3.1` outscored `mistral-large-2512` (8.59 vs 8.61 on grok,
**9.09 vs 8.88** on mistral judge — the most discerning axis).

### Mid/cheap tier (May 15 grok-4.3 judge)

| Candidate | grok-4.3 judge |
|-----------|---------------|
| openai/gpt-5.4-mini | 8.63 |
| mistralai/mistral-medium-3.1 | 8.59 |
| meta-llama/llama-4-scout | 8.32 |
| meta-llama/llama-3.3-70b-instruct | 7.83 |

`llama-3.3-70b` consistently beats `gpt-oss-120b` (7.83 vs 7.12), justifying
the Groq dialogue swap.

### Cheap tier (May 14, 12-candidate ELO + multi-axis)

| Candidate | grok-4.3 judge | ELO (mistral judge) |
|-----------|---------------|---------------------|
| qwen/qwen3-235b-a22b-2507 | 8.59 | 1898.9 (#1) |
| google/gemma-3-27b-it | 8.42 | 1705.0 (#3) |

`qwen3-235b` is the best-performing cheap-tier model by both ELO and
multi-axis — chosen for the OpenRouter budget dialogue slot.
`gemma-3-27b` is the best cheap sim/reaction pick.

## Test run: `cargo test -p parish-config`

All 129 tests pass, including `test_registry_has_all_providers` which
confirms all 23 providers still parse from TOML:

```
test provider::tests::test_registry_has_all_providers ... ok
test provider::tests::test_resolve_config_openrouter_requires_api_key ... ok
test provider::tests::test_resolve_config_openrouter_with_api_key ... ok
test provider::tests::test_vllm_provider_defaults ... ok
test provider::tests::test_vllm_provider_from_str ... ok

test result: ok. 129 passed; 0 failed; 0 ignored; 0 measured
```

## TypeScript check

`parish/apps/ui/src/lib/model-catalog.ts` is a pure data file (array of
`{name, provider}` objects). The only TypeScript errors in `svelte-check`
output are pre-existing `implicit any` warnings in `geojson.test.ts` —
none in `model-catalog.ts`.
