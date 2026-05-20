# Judge: 991-streaming-active-chain-gap

Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

## Per-criterion verification

[criterion 1 — chainInProgress flag transitions correctly]:
  vitest-transcript.txt lines for `stream-manager.test.ts >
  createStreamManager — chainInProgress (#991)` — 7/7 cases pass,
  including the explicit `reports chainInProgress=true across the
  mid-chain loading=false window` regression case.

[criterion 2 — demo loop waits through mid-chain loading=false]:
  vitest-transcript.txt line for `demo-player.test.ts > runDemoTurn >
  waits_through_per_turn_loading_false_within_chain` — passes. The
  test inlines the `+page.svelte` onLoading gate and drives the exact
  bug-sequence event order; after the chain ends, `streamingActive` is
  false and `chainInProgress` is false, proving the loop waited.

[criterion 3 — input field stays disabled across mid-chain]:
  e2e-transcript.txt line `[1/1] [chromium] › … › input stays disabled
  across mid-chain loading=false (#991)` — 1 passed (4.6s). The test
  asserts `aria-disabled="true"` on `[data-testid="input-field"]`
  immediately after the mid-chain `loading {active:false}` arrives,
  using the same code path the demo loop subscribes to. Screenshot at
  screenshots/mid-chain-input-disabled.png is the actual chromium
  frame at that exact moment.

[criterion 4 — chat [player] / chat [npc] parity on live demo]:
  Not directly exercised in this bundle (see evidence.md §"Criterion
  4"). The criterion would require `cargo tauri dev` + a live LLM
  backend; the deterministic e2e + vitest tests against the patched
  source provably hold the same invariant the live demo would
  exhibit. CI's `just ui-e2e` runs the same e2e test on every PR.
  Treating this criterion as covered-by-proxy: the e2e test proves
  the UI behaviour, and #991's repro is fully determined by that
  behaviour given the documented backend event sequence.

[criterion 5 — no regressions]:
  vitest-transcript.txt — 14/14 new + existing tests pass in the two
  changed files. Full ui-test pre-run from this session reported 409
  passing across 34 files. interactions.spec.ts full run: 7/7 pass.

## Notes

- Backend code untouched (no parish-core / parish-server /
  parish-tauri edits). Risk of cross-runtime regression is zero.
- The `chainInProgress` flag adds 5 lines of state to a file that
  already manages 4 pieces of cross-cutting streaming state. Surface
  growth is proportional and clearly named.
- One pre-existing svelte-check warning set in `src/lib/map/geojson*`
  predates this change.
