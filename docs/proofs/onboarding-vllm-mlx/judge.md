# Judge verdict — local-inference onboarding for vllm-mlx

Verdict: sufficient

Technical debt: clear

The PR closes a real first-run UX hole: before, a packaged
Parish.app silently expected the user to `uv tool install vllm-mlx`
+ run `huggingface-cli download` from a terminal. After, the user
clicks one button. Every layer that touches the wizard has an
automated test, and the bundle pipeline ran end-to-end on real
hardware across five clean-profile probes.

## What was claimed and verified

1. **Bundle pipeline materialises a working runtime.**
   `just build-vllm-mlx-bundle` produces a 356 MB compressed tarball
   that pip-installs vllm-mlx into a relocatable
   python-build-standalone runtime. The recipe runs cleanly on
   macos-arm64, `python3 -m vllm_mlx.cli serve --help` accepts every
   flag the Rust spawn passes, and the runtime survives relocation
   to `/tmp/pbs-relo` with `import vllm_mlx` resolving correctly.

2. **HuggingFace Hub download has progress events.**
   `HfModelDownloader::download_models` issues a two-pass flow
   (manifest + HEAD-for-size, then per-file `download_with_progress`)
   and reports bytes via the existing `SetupProgress` trait that
   SetupOverlay already binds to. Covered by 4 unit tests and 3
   wiremock integration tests.

3. **First-run UI surfaces the right fork.**
   `resolve_onboarding_choice` returns one of `Configured`,
   `LocalRecommended`, `LocalLowMem`, `LocalUnavailable`. The pure
   decision function is exercised by an 8-test matrix covering every
   short-circuit (prior onboarding, `PARISH_PROVIDER`, API key env,
   explicit provider) and every wizard variant. The Svelte UI then
   picks the loadout from live RAM: `ramGb >= 24` → two-slot,
   otherwise small-only — a 16 GB Mac OOMs on the 14B + 1.5B working
   set, so the fork separates "is local viable" (the Rust decision)
   from "which loadout fits" (the UI decision).

4. **Bundle resource path is wired into `tauri.conf.json`.**
   `parish/dist/vllm-mlx/python-runtime/` is declared as a bundle
   resource, the runtime probe in `resolve_bundled_vllm_mlx_paths`
   finds `<Resources>/vllm-mlx/python-runtime/bin/python3` on macOS,
   and `VllmMlxInvocation::resolve` dispatches to
   `python3 -m vllm_mlx.cli` instead of trying to exec a binary with
   a stale absolute shebang. A `.gitkeep` keeps the resource path
   valid in dev builds that skip the bundle step.

## Live probe summary

Five clean-profile probes drove the wizard via the MCP bridge
without manual clicks:

| Probe | Loadout    | What it proved                                                       |
|-------|------------|----------------------------------------------------------------------|
| 1     | small-only | Wizard runs → vllm-mlx spawns → `/v1/chat/completions` returns       |
| 2     | small-only | Live NPC dialogue (Tommy O'Brien at the Crossroads) through the bundled server, period Hiberno-English, references real NPCs by name |
| 3     | small-only | Wizard now spawns vllm-mlx without a relaunch (full post-gate bootstrap pipeline runs inside the wizard) |
| 4     | small-only | Tier 2 / Tier 3 JSON-parse storm silenced — 12+/30 s → 0             |
| 5     | two-slot   | 14B Dialogue + 1.5B Intent + simulator Sim/Reaction; Brigid replies with a marshmallow-root remedy and a Gaeilge sentence; Tier 2 storm: 0 |

Each probe ran from a fresh `HOME` / `PARISH_USER_CONFIG_DIR` so the
gate fires on a clean slate.

## Bugs caught + fixed during probes

1. `python -m vllm_mlx` had no `__main__` → switched to
   `python -m vllm_mlx.cli` (commit `246afe8f`).
2. `python -m venv` baked absolute build-host paths → switched to
   pip-into-runtime, dropped the venv layer (commit `1f978447`).
3. `handle_set_provider_config` aborted on a keychain platform error
   during a keyless local-provider wipe → tolerated with a warn log
   (commit `fd1be019`).
4. Wizard's persisted `parish.toml` never re-read at startup →
   `provider_config_from_env` now layers it under env vars.
5. `PARISH_HF_HOME` set only in-process during the wizard → startup
   re-seeds it from `<user_config_dir>/models/`.
6. Wizard emitted `setup-done` with the engine still inert → wizard
   now runs the same post-gate bootstrap pipeline `run()` does for
   returning users.
7. Tier 2 / Tier 3 JSON-parse storm on small-only → routed
   Sim+Reaction to the in-process simulator, fixed the simulator's
   JSON-detection shim, fixed a latent `intent_json_for`
   word-boundary bug.
8. Tier 3 boot-time "race" was a missed shim marker — `build_tier3_prompt`
   says "Respond with JSON" (no "a") + embeds a `{"updates":[…]}`
   schema, but the shim only matched "Respond with a JSON" /
   "JSON object". Added the missing markers and a regression test.
9. Bundled vllm-mlx orphaned to launchd on Cmd+Q → hooked
   `RunEvent::ExitRequested` to call `runtime_processes.stop()`
   while the tokio runtime is still alive (Drop on `AppState` was a
   catch-all but races runtime teardown).

## Wizard hardening

- Feature flag `local-inference-onboarding` (AGENTS.md rule #6),
  default-on, documented in `docs/features.md`.
- `AppState::wizard_in_flight: AtomicBool` idempotency guard so a
  duplicate POST while downloading drops cleanly. RAII clears the
  flag on every exit.
- Every failing exit emits `setup-done(success=false)` so the
  SetupOverlay drops out of the spinner with a real error.
- Three pinning tests so the simulator's JSON routing, the Tier 2
  contract through simulator, and the intent word-boundary fix
  can't silently regress.

## Technical debt

Clear. No placeholder macros, no "phase 2" deferrals. Every code
path the wizard exercises has a real implementation. Bundle recipe
materialises a real artifact (verified by hash). Download is real HF
Hub HTTP via `hf-hub` — not a stub. Onboarding decision logic is
pure and exhaustively tested. The 16 GB minimum and the per-category
routing policies it relies on were landed in earlier commits
(`53650e87`, `955e966e`) and have their own proof bundles under
`docs/proofs/local-perf/`.

The four shipping gaps the post-probe audit flagged — Tier 3 boot
parse-fail, vllm-mlx graceful shutdown on Cmd+Q, dev-mode fallback
probe, and flag discoverability — are all closed.

## What this verdict does NOT cover

Apple Developer codesigning + notarization are out of scope —
documented in the original plan and tracked separately. End users
will see a Gatekeeper warning on first launch until that lands.
Bundle hash verification at unpack, Linux/Windows bundling, and
auto-update of the model cache are also explicit non-goals.
