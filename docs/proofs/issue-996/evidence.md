# Evidence: issue-996

Evidence type: live gameplay transcript

The "live" component is the harness smoke run at `transcript.txt:15-22`,
which exercises the modified `vllm.toml` in a real engine boot. The unit
test at `transcript.txt:1-12` exercises the schema round-trip through the
production code paths (`Provider::preset_base_url`,
`GameConfig::fill_missing_models_from_presets`, `GameConfig::vllm_extra_slots`).

`parish-config` is not in the live-proof tier set (rule #10 path matrix), but
the bundle ships both signals anyway for defense-in-depth.

## Files changed

- `parish/crates/parish-config/providers/vllm.toml` — added
  `[presets.base_urls]` block pinning each category to its slot URL +
  documentation of the three-slot layout.
- `parish/crates/parish-core/src/ipc/config.rs` — added regression test
  `vllm_preset_supplies_per_category_base_url` covering the schema +
  fill-presets + extra-slots round-trip.
- `parish/testing/fixtures/play_issue-996.txt` — harness smoke fixture
  confirming the engine still parses the modified TOML.

## Criterion-to-line mapping

### Criterion 1 — `vllm.toml`'s recommended preset declares a `[presets.base_urls]` block

The file now contains the block at lines 24-28 of `vllm.toml`:

```toml
[presets.base_urls]
dialogue = "http://localhost:8000"
simulation = "http://localhost:8001"
intent = "http://localhost:8002"
reaction = "http://localhost:8001"
```

### Criterion 2 — `Provider::preset_base_url` returns the expected URL per category

`transcript.txt:10` shows the test passing:

```
test ipc::config::tests::vllm_preset_supplies_per_category_base_url ... ok
```

The test body asserts (`parish/crates/parish-core/src/ipc/config.rs`, new
test in `tests` mod):

- `vllm.preset_base_url(Dialogue) == Some("http://localhost:8000")`
- `vllm.preset_base_url(Simulation) == Some("http://localhost:8001")`
- `vllm.preset_base_url(Intent) == Some("http://localhost:8002")`
- `vllm.preset_base_url(Reaction) == Some("http://localhost:8001")`

### Criterion 3 — `fill_missing_models_from_presets` populates `category_base_url` for all four roles

Same passing test asserts on a `GameConfig { provider_name: "vllm",
base_url: "http://localhost:8000", .. }`:

- `category_base_url[Dialogue]    == ":8000"`
- `category_base_url[Simulation]  == ":8001"`
- `category_base_url[Intent]      == ":8002"`
- `category_base_url[Reaction]    == ":8001"`
- `category_model[Dialogue]       == Qwen/Qwen3-14B`
- `category_model[Simulation]     == Qwen/Qwen3-8B`
- `category_model[Intent]         == Qwen/Qwen3-4B`
- `category_model[Reaction]       == Qwen/Qwen3-8B`

### Criterion 4 — `vllm_extra_slots` emits three slots, dialogue elided

Same passing test asserts `slots.len() == 3` after setting `model_name =
Qwen3-14B` (so the dialogue slot matches the base). It then asserts the
returned URL/model pairs include `(:8001, 8B)`, `(:8002, 4B)`, and that
two of the three slots are the shared `(:8001, 8B)` — i.e. simulation +
reaction collapse onto the same backing process once
`VllmProcess::ensure_slots` dedupes downstream.

### Criterion 5 — Engine still parses the modified TOML and boots a headless session

`transcript.txt:18-22` shows the harness fixture emitting clean JSON for
`/status`, `look`, `/time`, `/npcs`, `/quit`. The session started, ran all
five commands, and exited cleanly. No parse error, no panic. The TOML
schema change is binary-compatible with the existing engine boot path.

## Why not a live LLM-call demo

The Linux/Windows `vllm` provider runs CUDA/ROCm hardware that this
worktree (macOS) doesn't have. Reproducing the 404 storm in-process would
require Linux + a real vllm server. The proof-of-correctness here is the
schema round-trip — every code path the runtime would take from preset →
`GameConfig` → `VllmSlot` list is exercised, and the same code is used by
`vllm_mlx.toml` (already proven on Apple Silicon in PR #990). The TOML
edit is a config port, not a logic change; the underlying machinery was
landed and verified by #990.
