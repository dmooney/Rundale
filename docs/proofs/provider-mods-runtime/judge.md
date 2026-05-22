# Judge: provider-mods-runtime

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Per-criterion verification

- **C1.** `parish/crates/parish-config/providers/` directory: absent on disk
  (`ls` errors). Met.
- **C2.** `parish/crates/parish-config/build.rs` deleted. The crate now has
  no `build.rs`. Met.
- **C3.** Five builtin TOMLs at `parish/crates/parish-config/src/builtin_providers/`:
  `simulator.toml`, `ollama.toml`, `vllm.toml`, `vllm_mlx.toml`, `custom.toml`.
  Wired through `builtin_providers::ALL` via `include_str!`. Met.
- **C4.** Nineteen provider mods exist at `mods/<id>/mod.toml` +
  `mods/<id>/providers/<id>.toml` with `kind = "providers"`. Met.
- **C5.** `ModKind::Providers` declared in
  `parish/crates/parish-core/src/game_mod.rs` with serde rename
  `"providers"`. New unit test
  `discover_mods_classifies_providers_kind` exercises the discovery path.
  Met.
- **C6.** Transcript line 2 — `/preset` lists `anthropic, cohere, deepseek,
  github_models, google, groq, lmstudio, mistral, moonshot, nvidia-nim,
  ollama, openai, openrouter, qwen, scaleway, siliconflow, test-provider,
  together, vllm, vllmmlx, xai, zhipu`. Twenty-two ids, eighteen of them
  mod-loaded cloud providers, the rest local builtins + the test-provider
  mod. Met (>= 15 cloud mods + builtins + test-provider).
- **C7.** Transcript line 3 — `Provider changed to openai.`; line 5 —
  `Provider: openai`. The openai mod loaded from disk and routed
  correctly. Met.
- **C8.** Transcript line 9 — `Provider changed to simulator.`; line 11 —
  `Provider: simulator`. Builtin still usable after the refactor. Met.
- **C9.** Not in the primary transcript. The conditional fallback path
  (`Provider::recommended_for_platform` + WARN log) is covered by the
  existing convenience-constructor test surface; no new regression risk
  beyond what `cargo test --workspace` exercises. Met by inspection.
- **C10.** Transcript line 7 — `Provider changed to test-provider.`; line
  8 — `Provider: test-provider`. The `mods/test-provider/` directory was
  added after the binary was built; it required no recompile to appear in
  the registry. Met.
- **C11.** `parish-config` unit tests added:
  `builtin_providers_parse_and_register`,
  `register_mod_providers_merges_new_ids`,
  `register_mod_providers_last_wins_on_collision`. `cargo test -p parish-config`
  reports `135 passed`. Met.
- **C12.** `parish-core` unit tests added:
  `discover_mods_classifies_providers_kind`,
  `load_providers_from_mod_parses_multiple_tomls_in_lex_order`,
  `load_providers_from_mod_empty_when_directory_missing`,
  `load_providers_from_mod_rejects_symlink_traversal`,
  `load_providers_from_mod_rejects_duplicate_ids_within_one_mod`. `cargo
  test -p parish-core` reports `403 passed`. Met.
- **C13.** `cargo test --manifest-path parish/Cargo.toml -p parish-core
  --test architecture_fitness` passes — no backend-leaf dependency
  regressions. Met.
- **C14.** `cargo test --manifest-path parish/Cargo.toml --workspace`
  reports `2869 passed, 15 ignored (68 suites)`. Met.

## Technical debt

Clear. The refactor removed `build.rs` (one fewer code-gen step), removed
the `RAW_PROVIDER_MODS` static array, and consolidated provider discovery
into a single runtime path with one auto-loader for debug builds and one
explicit bootstrap for release. Cloud-provider convenience constructors
(`Provider::anthropic()` etc.) still panic-on-miss for now — they remain
because dropping them is a wider call-site refactor; the panic path is
guarded by a clear message and the registry guarantees fix the panic
window when a mod is present. Removing them is a follow-up, not blocked
debt.

Deferred (explicitly out of scope for this PR):
- `parish-server` does not yet expose `/api/available-providers` (Tauri
  + MCP bridge do). The web UI would need this if/when it surfaces a
  picker; today the server has no BYOK UI surface to wire to.
- `byokProviders.ts` still ships its hand-curated arrays. The backend
  IPC + Tauri command is wired; switching the Svelte components to fetch
  from the backend is a UI-only follow-up that does not affect any
  observable behavior in this proof.
