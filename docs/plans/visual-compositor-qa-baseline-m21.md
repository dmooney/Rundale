# Visual Compositor QA Baseline M21 Plan

1. Add M21 acceptance criteria, deterministic `/scene` fixture, design note, and implementation plan.
2. Extend `parish/apps/visual/scripts/audit-scene-atoms.mjs` with visible-content metrics for PNG atoms and scene-level contribution summaries.
3. Harden the audit so it rejects SVG/missing assets, blank or near-blank PNG atoms, suspicious non-allowlisted full-stage atoms, and mis-sized full-stage `shadow`/`lighting` overlays.
4. Add visual-client tests for the contribution metrics and the new failure cases without adding visible debug UI.
5. Add an M21 proof script that records atom audit JSON and reuse/extend the atom-only browser proof to capture live telemetry/screenshots.
6. Verify with:
   - `npm --prefix parish/apps/visual run test`
   - `npm --prefix parish/apps/visual run check`
   - `npm --prefix parish/apps/visual run build`
   - `npm --prefix parish/apps/visual run audit:atoms`
   - `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/fixtures/play_visual-compositor-qa-baseline-m21.txt`
   - live browser proof
   - `bash parish/scripts/agent-check.sh --source=local`
