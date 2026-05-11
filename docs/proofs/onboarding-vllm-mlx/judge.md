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

## What this verdict does NOT cover

A full live probe of the packaged .app requires fixing the
unrelated `@tauri-apps/api` (v2.10.1) vs `tauri` Rust crate
(v2.11.1) version mismatch that `main` carries today.
`cargo tauri build` refuses to run with that mismatch in place.
The mismatch is independent of this PR and was present on
origin/main before any of these commits. The manual-probe
checklist in `evidence.md` documents the steps that need to
happen on the user's box once that gate is unblocked.

Apple Developer codesigning + notarization are also out of scope —
documented in the original plan and tracked separately. End users
will see a Gatekeeper warning on first launch until that lands.
