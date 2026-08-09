# Frontier local-dialogue qualification campaign — 2026-07-26

This campaign applies the production dialogue-promotion funnel to newly
available local models. A candidate stops at the first decisive failure.
Deterministic preflight failures do not proceed to paid or subjective
judgment.

## Fixed qualification contract

- Host profile: Apple Silicon, 48 GB unified memory.
- Dataset manifest:
  `feb43d4bf80dae70b1cda3f506f58db7a26602a7a24d2a42f3453cf3726ec912`.
- Live preflight: 12 scripted turns through the shared Parish server,
  canonical parser, and production dialogue guards.
- Preflight requirements: one contract-valid dialogue turn per call, no
  zero-tolerance fabrication signal, and no more than 10% guard intervention.
- Survivors advance to the frozen holdout, multi-turn evaluation, 500-call
  reliability/guard soak, and cold/warm performance measurement defined by
  `promptfoo/config/dialogue_promotion.json`.

## Results

| Order | Exact candidate | Furthest stage | Evidence | Decision |
| ---: | --- | --- | --- | --- |
| 1 | Google Gemma 4 26B A4B IT, official Q4 GGUF | 12-turn live preflight | 12/12 contract-valid full JSON; 12/12 `mood_register_guard`; 29.42 s total | Reject: 100% guard intervention exceeds the 10% ceiling |
| 2 | `mlx-community/diffusiongemma-26B-A4B-it-4bit` | Runtime load | Weights downloaded; vLLM-MLX failed with `Model type diffusion_gemma not supported` | Reject: cannot run on the supported production server |
| 3 | `mlx-community/Qwen3.6-35B-A3B-4bit` | 12-turn live preflight | 10/12 contract-valid; 10 full JSON and 2 recovered; 11/12 guarded (10 verbosity, 8 mood/register); 119.80 s total | Reject: reliability and guard ceilings both fail |
| 4 | Kimi Linear 48B A3B Instruct, `DhruvalLabs/...:Q4_K_M` GGUF | 12-turn live preflight | MLX package required disabled remote code; safe GGUF fallback loaded; 9/12 contract-valid; 9 full JSON and 3 recovered; 12/12 guarded (11 verbosity, 12 mood/register); 27.66 s total | Reject: reliability and guard ceilings both fail |
| 5 | `poolside/Laguna-XS-2.1-GGUF:Q4_K_M` | Runtime load | Official weights downloaded; llama.cpp failed with `unknown model architecture: 'laguna'` | Reject: cannot run on the supported production server |
| 6 | `mlx-community/Qwen3.6-27B-4bit` | 12-turn live preflight | 8/12 contract-valid; 8 full JSON and 4 recovered; 12/12 guarded (12 verbosity, 10 mood/register); 420.21 s total; observed server throughput commonly 4.7–11.5 tok/s | Reject: reliability, guard, and throughput bars fail |
| 7 | `prism-ml/Ternary-Bonsai-27B-mlx-2bit` | 12-turn live preflight | 12/12 contract-valid full JSON; 10/12 verbosity interventions; 178.15 s total; observed server throughput commonly 4.4–8.3 tok/s | Reject: guard and throughput bars fail |
| 8 | `mradermacher/Gemma-4-26B-A4B-StyleTune-V2-GGUF:Q4_K_M` | 500-call live soak, decisively stopped at 259 | Preflight: 12/12 contract-valid full JSON with zero guards. Soak: 259/259 contract-valid and profiled, zero transport errors, 54 guard interventions (47 mood/register, 7 grounding, 4 polish reason activations) | Reject: even 241 additional clean calls would leave 54/500 = 10.8%, above the 10% guard ceiling |
| 9 | `mlx-community/GLM-4.7-Flash-4bit` | 12-turn live preflight | 0/12 contract-valid; 12 raw-text parse dispositions; 12/12 verbosity interventions; 50.17 s total. The server's JSON enforcer became stuck on the model's `</think>` output and disabled constrained decoding per request | Reject: reliability and guard ceilings both fail |
| 10 | `mlx-community/Nemotron-Cascade-2-30B-A3B-4bit` | 12-turn live preflight | 0/12 contract-valid; 12 raw-text parse dispositions; 8/12 guard interventions (6 canonical repetition, 3 verbosity reason activations); 37.36 s total. Its `</think>` output likewise caused the JSON enforcer to disable constrained decoding | Reject: reliability and guard ceilings both fail |

Immutable turn-level, summary, and runtime-failure artifacts are under
`runs/2026-07-26/`. DiffusionGemma produced no dialogue turns because the
server could not load the model. Qwen's repository is marked multimodal, but
the installed vLLM-MLX runtime loaded its language model successfully; this
demonstrates that the older metadata-only preflight must not reject every
multimodal repository without checking current runtime capability.

StyleTune's first two soak attempts exposed harness defects rather than model
defects and are retained with `invalid-` filenames. Parish's production
session cookie is `Secure`, which Python's default cookie jar would not return
to a loopback HTTP server; that silently created one session per command until
the 50-session admission cap. After preserving session continuity, scripted
or model-generated farewells could still close a conversation before the next
counted command. The corrected harness permits the `Secure` cookie only on
loopback, requires a real dialogue request profile for every denominator row,
and reacquires another NPC before retrying a no-inference command. The 259-row
early-stop artifact is the first StyleTune soak evidence valid under both
conditions.

## Promotion state

No candidate in this campaign qualified for the production registry.
StyleTune was the only candidate to clear the live preflight, but its
production soak made the guard-budget failure mathematically irreversible.
A structurally valid response is not sufficient when the player-facing result
depends on deterministic rewriting.
