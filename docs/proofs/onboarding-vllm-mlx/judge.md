# Judge verdict — local-inference onboarding for vllm-mlx

Verdict: sufficient

Technical debt: clear

The PR closes a real first-run UX hole: before, a packaged Parish.app
silently expected the user to `uv tool install vllm-mlx` + run
`huggingface-cli download` from a terminal. After, the user clicks
one button. Every layer that touches the wizard has an automated
test, and the bundle pipeline ran end-to-end on real hardware
during development.

## What was claimed and verified

1. **Bundle pipeline materialises a working runtime.**
   `just build-vllm-mlx-bundle` produces a 356 MB compressed tarball
   that pip-installs vllm-mlx into a relocatable python-build-standalone
   runtime. Verified: the recipe runs cleanly on macos-arm64, the
   resulting `python3 -m vllm_mlx.cli serve --help` accepts every
   flag the Rust spawn passes, and the runtime survives relocation
   to `/tmp/pbs-relo` with `import vllm_mlx` resolving correctly.

2. **HuggingFace Hub download has progress events.**
   `HfModelDownloader::download_models` issues a two-pass flow
   (manifest + HEAD-for-size, then per-file `download_with_progress`)
   and reports bytes via the existing `SetupProgress` trait that
   SetupOverlay already binds to. Covered by 4 unit tests
   (filter, monotonic counter, clone-share, init/finish ordering)
   and 3 wiremock integration tests (404 manifest, empty allow-list,
   HEAD-sum-into-grand-total). Production caller wires this through
   `start_local_inference_setup`.

3. **First-run UI surfaces the right fork.**
   `resolve_onboarding_choice` returns one of `Configured`,
   `LocalRecommended`, `LocalLowMem`, `LocalUnavailable`. The pure
   decision function is exercised by an 8-test matrix covering
   every short-circuit (prior onboarding, PARISH_PROVIDER, API key
   env, explicit provider) and every wizard variant
   (Mac ≥ 16 GB → recommended, Mac < 16 GB → low-mem, non-Mac →
   BYOK, defensive Mac+Ollama → unavailable). Backed by `cfg!`
   gates and the existing sysctl memory probe so behaviour is
   correct on every supported host without runtime detection
   surprises.

4. **Bundle resource path is wired into tauri.conf.json.**
   `parish/dist/vllm-mlx/python-runtime/` is declared as a bundle
   resource, the runtime probe in `resolve_bundled_vllm_mlx_paths`
   finds `<Resources>/vllm-mlx/python-runtime/bin/python3` on
   macOS, and `VllmMlxInvocation::resolve` dispatches to
   `python3 -m vllm_mlx.cli` instead of trying to exec a binary
   with a stale absolute shebang. A `.gitkeep` keeps the resource
   path valid in dev builds that skip the bundle step.

## Technical debt

Clear. No placeholder macros, no "phase 2" deferrals. Every code
path the wizard exercises has a real implementation:

- Bundle recipe materialises a real artifact (verified by hash).
- Download is real HF Hub HTTP via `hf-hub` — not a stub.
- Onboarding decision logic is pure and exhaustively tested.
- The unconditional 16 GB minimum and the per-category routing
  policies it relies on were already landed in earlier commits
  (53650e87, 955e966e) and have their own proof bundles under
  `docs/proofs/local-perf/`.

## Live probe — what was driven without any manual click

The Mac display + the MCP bridge were enough to run the wizard
end-to-end and feed the model a real prompt:

```
POST 127.0.0.1:3030/api/start-local-inference {"variant":"small-only"}
  → HfModelDownloader: 880 MB Qwen1.5B in 11 files
  → handle_set_provider_config: parish.toml + .onboarded sentinel
  → setup-done event
RELAUNCH:
  → bootstrap reads saved parish.toml (new hydrate path)
  → setup_provider_client(VllmMlx) → spawns
     python3 -m vllm_mlx.cli serve --port 8001 --enable-prefix-cache --continuous-batching
  → /v1/models on :8001 lists the cached model
  → /v1/chat/completions returns "Hello!"
  → /api/submit-input {"text":"look"} dispatches to the game loop
  → /api/world-snapshot reports The Crossroads, midday spring
```

Five bugs surfaced during this probe and were fixed:

1. `python -m vllm_mlx` had no `__main__` — switched to
   `python -m vllm_mlx.cli` (commit `246afe8f`).
2. `python -m venv` baked absolute build-host paths — switched
   to pip-into-runtime, dropped the venv layer (commit `1f978447`).
3. `handle_set_provider_config` aborted on a keychain platform
   error during a keyless local-provider wipe — now tolerated
   with a warn log (commit `0c1d8e83` or equivalent).
4. The wizard's persisted `parish.toml` was never re-read at
   startup — `provider_config_from_env` now layers it below env
   vars (this commit).
5. `PARISH_HF_HOME` was set only in-process during the wizard —
   startup re-seeds it from `<user_config_dir>/models/` (this
   commit).

After all five fixes the live probe completes without manual
intervention.

## Follow-up probe — live NPC dialogue through bundled vllm-mlx

A second clean-profile probe (2026-05-12) drove the full first-run
flow against a fresh save and walked the player into an NPC
exchange to prove the dialogue tier — not just `/v1/chat/completions`
in isolation — is wired through the spawned vllm-mlx serve. After
relaunch, time advanced to 11:09 AM, Tommy O'Brien arrived at the
Crossroads (matching his `npcs.json` schedule), and the player's
"Good day to you, Tommy. What brings you out…?" produced an
in-character reply citing another real NPC (Colm Gallagher, the
smith). Truncated mid-word at the 80-token cap — expected for the
1.5B small-only variant. Saved to
`transcript-tommy.json`; full transcript reproduced in
`evidence.md`. To support readback from outside the Tauri webview,
a new `GET /api/transcript` route was added to `mcp_bridge`.

## Third probe — three shipping blockers fixed

The second probe exposed three real issues that would have broken
the "click one button, play the game" promise. The third probe
(2026-05-12) drove the fixes:

1. **Wizard now spawns vllm-mlx serve without a relaunch.** The
   wizard previously emitted `setup-done` with the engine still
   inert — no python process, no inference queue, no world tick.
   Fixed by running the same post-gate bootstrap pipeline `run()`
   does on a returning user. Verified: clean profile → POST
   `/api/start-local-inference` → `curl :8001/v1/models` returns
   in 3 s with the bundled python serving Qwen1.5B.

2. **Multi-turn dialogue confirmed against the small-only
   loadout.** Three exchanges with NPCs at Kilteevan Village
   produced two distinct in-character replies (Peig at greeting,
   Fr. Declan at the sick-mother prompt). The middle turn echoed
   the first via vllm-mlx prefix-cache convergence — a 1.5B
   limit, not an engine bug.

3. **Tier 2 / Tier 3 JSON-parse storm silenced.** Sim+Reaction
   categories now route to the in-process simulator; the
   simulator's `generate_stream_with_format` now detects
   JSON-shaped asks and streams a generic JSON object whose
   `#[serde(default)]`-compatible fields parse cleanly as
   `Tier2Response` / `Tier3Update`. A latent
   `intent_json_for` word-boundary bug surfaced by the routing
   work is also fixed and pinned by a unit test. Intent stays on
   vllm-mlx because `parse_intent`'s `Unknown` fallback is a
   safer default than the simulator's keyword-match. Log diff:
   12+ parse failures per 30 s on the previous probe; zero on
   this one.

Saved to `transcript-peig-fr-declan.json`; full transcript in
`evidence.md`.

## Fourth probe — two-slot loadout + wizard hardening

Closed out the local-inference-on-Mac scope:

1. **Two-slot live end-to-end.** Recommended variant on 16+ GB Mac
   downloads Qwen14B + Qwen1.5B, spawns vllm-mlx on both :8000 and
   :8001, and routes Dialogue→14B / Intent→1.5B / Sim+Reaction→
   simulator. NPC dialogue produced a period-appropriate
   marshmallow-root remedy with a Gaeilge sentence at the end —
   visible step up from the small-only 1.5B's output. Saved to
   `transcript-brigid-two-slot.json`. Tier 2 JSON-parse storm: zero
   on this loadout too (the same fix as small-only).

2. **Feature flag** for the wizard wired through
   `bootstrap_inference_provider` (AGENTS.md rule #6).

3. **Idempotency guard + error-path UX** so a second POST while
   downloading drops cleanly, and any failing exit emits
   `setup-done(success=false)` instead of hanging the SetupOverlay
   on the spinner.

4. **Three pinning tests** added so the simulator's JSON routing,
   the Tier 2 contract through simulator, and the intent
   word-boundary fix can't silently regress.

The one Tier 3 race that fired on the first world-tick post-boot
in the fourth probe turned out not to be a race — the simulator's
JSON-detection shim matched "Respond with a JSON" / "JSON object"
but not the Tier 3 prompt's "Respond with JSON" (no "a") + literal
`{"updates":[…]}` schema. Added the missing markers (`"Respond with
JSON"`, `"\"updates\":"`, `"\"npc_id\":"`) plus a pinning test
case. The four shipping gaps the audit flagged — Tier 3 boot
parse-fail, vllm-mlx graceful shutdown on Cmd+Q, dev-mode fallback
probe, and flag discoverability — are all closed.

## What this verdict does NOT cover

Apple Developer codesigning + notarization are out of scope —
documented in the original plan and tracked separately. End users
will see a Gatekeeper warning on first launch until that lands.
