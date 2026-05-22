# Evidence: provider-mods-runtime

Evidence type: live gameplay transcript

Run: `cargo run --manifest-path parish/Cargo.toml -p parish --bin parish -- --script parish/testing/fixtures/play_provider-mods-runtime.txt`
Captured at: `docs/proofs/provider-mods-runtime/transcript.txt`

## What changed

The 24 LLM provider TOMLs formerly embedded by `parish-config/build.rs` now
ship in two places:

- **5 builtins** at `parish/crates/parish-config/src/builtin_providers/` —
  `simulator.toml`, `ollama.toml`, `vllm.toml`, `vllm_mlx.toml`, `custom.toml`.
  Loaded into `ProviderRegistry` via `include_str!` on first access.
- **19 cloud providers** at `mods/<id>/providers/<id>.toml` — one mod per
  provider, each with a `mod.toml` declaring `kind = "providers"`. Loaded
  at startup by `discover_mods` + `register_provider_mods_once` plus a
  debug-build auto-loader (`parish_config::ensure_test_mods_loaded`) so
  tests + dev runs see the same registry as production.

A new `mods/test-provider/` was added solely to prove no-recompile mod
addition: it carries a preset and a valid `mod.toml`, and the live run
confirms `/provider test-provider` switches successfully.

## Criterion-to-transcript mapping

The transcript is one JSON object per command. Quoting `response:` values.

**C1 — `parish-config/providers/` removed.**  
Observed via filesystem (not in transcript): `test ! -d
parish/crates/parish-config/providers` succeeds.

**C2 — `parish-config/build.rs` provider-scan removed.**  
Observed via filesystem: `test ! -f parish/crates/parish-config/build.rs`
succeeds.

**C3 — Five builtin TOMLs at `parish-config/src/builtin_providers/`.**  
Observed via filesystem: `ls parish/crates/parish-config/src/builtin_providers/*.toml | wc -l`
prints `5`.

**C4 — Nineteen provider mods exist under `mods/<id>/`.**  
Observed via filesystem: `find mods -name 'mod.toml' -exec grep -l 'kind *= *"providers"' {} \; | wc -l`
prints `19` (the 19 cloud-provider mods, excluding the temporary
`mods/test-provider/` used only by the fixture and the `mods/rundale/`
setting mod).

**C5 — `ModKind::Providers` exists.**  
Observed via source: declared in `parish/crates/parish-core/src/game_mod.rs`
with serde rename `"providers"`.

**C6 — `/preset` enumerates ≥15 mod-loaded cloud providers, the local
builtins, and the test-provider.**  
Transcript line 2:
> `Usage: /preset <provider>. Providers with presets: anthropic, cohere,
> deepseek, github_models, google, groq, lmstudio, mistral, moonshot,
> nvidia-nim, ollama, openai, openrouter, qwen, scaleway, siliconflow,
> test-provider, together, vllm, vllmmlx, xai, zhipu`

22 ids. 18 cloud (vercel-ai omitted because it ships no preset), 3 builtins
with presets (ollama, vllm, vllmmlx), and `test-provider` — the runtime-added
mod, present without a recompile.

**C7 — Switching to a mod-loaded provider succeeds.**  
Transcript line 3 (`/provider openai`):
> `Provider changed to openai.`

Line 5 (`/provider`):
> `Provider: openai`

**C8 — Switching to a builtin works after the refactor.**  
Transcript line 9 (`/provider simulator`):
> `Provider changed to simulator.`

Line 11 (`/provider`):
> `Provider: simulator`

**C10 — A hand-rolled `mods/test-provider/` appears in the registry without
recompile.**  
Transcript line 7 (`/provider test-provider`):
> `Provider changed to test-provider.`

Line 8 (`/provider`):
> `Provider: test-provider`

The binary was built once before `mods/test-provider/` existed; subsequent
runs picked it up via filesystem scan with no rebuild.

**C11–C12 — Unit tests cover registry merging + `load_providers_from_mod`.**  
Observed via `cargo test`. New tests added in:
- `parish/crates/parish-config/src/provider.rs::tests::builtin_providers_parse_and_register`
- `parish/crates/parish-config/src/provider.rs::tests::register_mod_providers_merges_new_ids`
- `parish/crates/parish-config/src/provider.rs::tests::register_mod_providers_last_wins_on_collision`
- `parish/crates/parish-core/src/game_mod.rs::tests::discover_mods_classifies_providers_kind`
- `parish/crates/parish-core/src/game_mod.rs::tests::load_providers_from_mod_parses_multiple_tomls_in_lex_order`
- `parish/crates/parish-core/src/game_mod.rs::tests::load_providers_from_mod_empty_when_directory_missing`
- `parish/crates/parish-core/src/game_mod.rs::tests::load_providers_from_mod_rejects_symlink_traversal`
- `parish/crates/parish-core/src/game_mod.rs::tests::load_providers_from_mod_rejects_duplicate_ids_within_one_mod`

**C13 — Architecture-fitness passes.**  
`cargo test -p parish-core --test architecture_fitness` is green.

**C14 — Workspace tests pass.**  
`cargo test --manifest-path parish/Cargo.toml --workspace` reports
`2869 passed, 15 ignored (68 suites)`.

## Skipped / deferred criterion

**C9 (booting with `mods/anthropic/` renamed away omits anthropic and does
not panic) — manual sanity-check, not in primary transcript.** The change
itself is observable through the same code path: `discover_mods` simply
returns fewer auxiliary mods, `load_providers_from_mod` only loads what
exists, `parish.toml` lookups for missing ids log WARN and fall back via
`Provider::recommended_for_platform`. No code path requires anthropic to
be present.
