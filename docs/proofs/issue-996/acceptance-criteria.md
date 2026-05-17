# Acceptance Criteria: issue-996

## Task

Port the canonical multi-slot loadout into the Linux/Windows `vllm` provider
preset. PR #990 added `[presets.base_urls]` to `vllm_mlx.toml` (Apple Silicon)
so each category routes to the slot where its preset model is actually loaded.
The sister TOML — `parish/crates/parish-config/providers/vllm.toml` — was
left behind, so `fill_missing_models_from_presets` auto-picks the preset
model (e.g. `Qwen3-8B`) but inherits the user-level base URL (`:8000`, where
only the 14B is loaded) → guaranteed 404 storm on Linux/Windows the moment a
user pulls main.

Fix: declare a three-slot loadout in `vllm.toml`, with each unique Qwen3 model
size pinned to its own port — `:8000` (14B / dialogue), `:8001` (8B / shared
by simulation + reaction), `:8002` (4B / intent). This is the layout the
issue author named as primary; it also round-trips cleanly through
`vllm_extra_slots` + `VllmProcess::ensure_slots`, because each unique model
gets its own slot key.

## Criteria

- `vllm.toml`'s recommended preset declares a `[presets.base_urls]` block
  pinning each category to its slot URL — observable via: `cat
  parish/crates/parish-config/providers/vllm.toml`.
- `Provider::from_str_loose("vllm")?.preset_base_url(cat)` returns the
  expected URL for each of the four `InferenceCategory` variants (dialogue
  `:8000`, simulation `:8001`, intent `:8002`, reaction `:8001`) — observable
  via: a new `parish-core` unit test that calls `preset_base_url`.
- `GameConfig { provider_name: "vllm", base_url: "http://localhost:8000", ..
  }.fill_missing_models_from_presets()` populates `category_base_url` for all
  four categories from the preset — observable via: a new `parish-core` unit
  test that asserts the four entries match.
- `vllm_extra_slots()` on that filled `GameConfig` emits three slots — sim
  `Qwen3-8B@:8001`, intent `Qwen3-4B@:8002`, reaction `Qwen3-8B@:8001` — the
  dialogue slot `Qwen3-14B@:8000` is the base and is correctly elided.
  Downstream dedup (`VllmProcess::ensure_slots`) collapses the duplicate 8B
  slot to two unique processes. Observable via: the same new unit test.
- Engine still parses the modified TOML and boots a headless session
  (regression guard for any TOML syntax slip) — observable via: the harness
  smoke fixture below loads and emits a non-error `/status` payload.

## Verification

1. Cargo test (primary signal — exercises the schema round-trip):

   ```sh
   cargo test --manifest-path parish/Cargo.toml -p parish-core \
     --lib ipc::config::tests::vllm_preset_supplies_per_category_base_url \
     -- --nocapture
   ```

   Expected: test passes. Output should show the four URL/model assertions
   succeed and `vllm_extra_slots` returning the 8B and 4B slots.

2. Harness smoke (parse-error guard — confirms engine still loads):

   ```sh
   cargo run --manifest-path parish/Cargo.toml -p parish-cli -- \
     --script parish/testing/fixtures/play_issue-996.txt
   ```

   Expected: `/status` JSON emitted, engine reaches end of script cleanly.

Capture both into `docs/proofs/issue-996/transcript.txt`.
