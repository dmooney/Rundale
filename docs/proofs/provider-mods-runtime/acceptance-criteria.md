# Acceptance Criteria: provider-mods-runtime

## Task

Replace the compile-time-embedded provider catalog with runtime-loaded provider
mods. Today, 24 LLM provider TOMLs live at `parish/crates/parish-config/providers/`
and are baked into the binary by `build.rs`. After this refactor:

- **Five providers stay hardcoded in `parish-config`** as builtins (engine-level,
  always available): `simulator`, `ollama`, `vllm`, `vllm_mlx`, `custom`. The first
  four manage local processes / model downloads; `custom` is the universal
  OpenAI-compat escape hatch and has no fixed identity.
- **Twenty providers become separate runtime mods** under `mods/<id>/` (one mod per
  provider). Discovery, parsing, and registration happen at startup via the existing
  `discover_mods()` pipeline plus a new `ModKind::Providers` variant.
- **UI enumerates providers from the backend** via a new `list_available_providers`
  IPC rather than from a hardcoded `byokProviders.ts` array.

After the change, a player or operator can drop a new provider TOML into a
hand-rolled `mods/<id>/providers/<id>.toml` and have it appear in the picker + be
selectable, all without recompiling the engine. A player who deletes one of the
shipped provider mods sees that provider disappear from the picker — no crash, no
panic. The five builtins remain available regardless of what's under `mods/`.

## Criteria

- **C1.** `parish/crates/parish-config/providers/` does not exist on disk after the
  refactor. — observable via: `test ! -d parish/crates/parish-config/providers`
- **C2.** `parish/crates/parish-config/build.rs` no longer scans a `providers/`
  directory (either deleted entirely or rewritten to do unrelated build work). —
  observable via: `! grep -q 'providers' parish/crates/parish-config/build.rs ||
  test ! -f parish/crates/parish-config/build.rs`
- **C3.** Five builtin TOMLs live in `parish/crates/parish-config/src/builtin_providers/`
  (`simulator.toml`, `ollama.toml`, `vllm.toml`, `vllm_mlx.toml`, `custom.toml`). —
  observable via: `ls parish/crates/parish-config/src/builtin_providers/*.toml | wc -l`
  prints `5`.
- **C4.** Nineteen provider mods exist at `mods/<id>/mod.toml` +
  `mods/<id>/providers/<id>.toml` for each of: anthropic, cohere, deepseek,
  github_models, google, groq, lmstudio, mistral, moonshot, nvidia-nim, openai,
  openrouter, qwen, scaleway, siliconflow, together, vercel-ai, xai, zhipu. —
  observable via:
  `find mods -name 'mod.toml' -exec grep -l 'kind *= *"providers"' {} \; | wc -l`
  prints `19`.
- **C5.** `ModKind::Providers` is a declared variant in
  `parish/crates/parish-core/src/game_mod.rs`. — observable via:
  `grep -q 'Providers' parish/crates/parish-core/src/game_mod.rs` and the type
  parses `kind = "providers"` from TOML.
- **C6.** With the full `mods/` directory present, the CLI fixture's `/preset` output
  enumerates ≥15 mod-loaded cloud provider IDs alongside the local builtins (ollama,
  vllm, vllmmlx) and the `test-provider` mod added solely to prove no-recompile
  registration. — observable via: the `/preset` response in the transcript lists
  these ids interleaved alphabetically.
- **C7.** Switching to a mod-loaded provider succeeds. `/provider openai` returns a
  success effect (not "unknown provider"). — observable via: the JSON `result`
  envelope after `/provider openai` contains `success` and no `error` field; the
  follow-up `/provider` shows `openai`.
- **C8.** Switching to a builtin still works after the refactor. `/provider simulator`
  reverts to the simulator. — observable via: post-switch `/provider` output shows
  `simulator`.
- **C9.** Removing one mod (e.g. `mods/anthropic/`) before booting omits it from
  `/preset` and does not panic. — observable via: separate run of the fixture against
  a temporarily renamed `mods/anthropic/` directory; transcript shows `anthropic` is
  absent and the run completes normally. (Logged in transcript as a second invocation
  block.)
- **C10.** A hand-rolled `mods/test-provider/` directory (created by the fixture
  setup) with a valid `mod.toml` + `providers/test-provider.toml` appears in `/preset`
  output without any code change or recompile. — observable via: transcript line
  matching `provider: test-provider`.
- **C11.** `cargo test -p parish-config` passes new unit tests covering
  registry-with-builtins-only, `register_mod_providers` merging, and last-wins
  collision behaviour with WARN logging.
- **C12.** `cargo test -p parish-core` passes new unit tests covering
  `load_providers_from_mod` (happy path + traversal-rejection) and `ModKind::Providers`
  parsing.
- **C13.** `parish/crates/parish-core/tests/architecture_fitness.rs` passes — no new
  `tauri`/`axum`/`tower`/`wry`/`tao` dependencies leak into `parish-config` or
  `parish-core`.
- **C14.** `just check` (fmt + clippy + tests) passes.

## Verification script

Run: `cargo run --manifest-path parish/Cargo.toml -p parish-cli -- --script
parish/testing/fixtures/play_provider-mods-runtime.txt`

Expected signals in output:

- After the initial `/status`: JSON shows a `Location: ...` line, confirming the
  game started.
- After `/preset`: the response lists, at minimum: `anthropic`, `cohere`,
  `deepseek`, `github_models`, `google`, `groq`, `lmstudio`, `mistral`, `moonshot`,
  `nvidia-nim`, `ollama`, `openai`, `openrouter`, `qwen`, `scaleway`, `siliconflow`,
  `test-provider`, `together`, `vllm`, `vllmmlx`, `xai`, `zhipu`. (`vercel-ai`,
  `simulator`, and `custom` ship without presets and are intentionally absent.)
- After `/provider openai`: response is `Provider changed to openai.`
- After the follow-up `/provider`: response is `Provider: openai`.
- After `/preset openai`: response is `Applied openai preset
  (Dialogue/Simulation/Intent/Reaction). ...` — proves the mod's TOML parsed.
- After `/provider test-provider`: response is `Provider changed to
  test-provider.` (Proves runtime-added mod is selectable without recompile.)
- After `/provider simulator`: response is `Provider changed to simulator.`
- After the final `/provider`: response is `Provider: simulator`.

The fixture is intentionally minimal — it exercises the *runtime-loaded provider
registry* end-to-end without depending on real cloud API keys. No real network
call is made; only registry enumeration and provider-id dispatch.
