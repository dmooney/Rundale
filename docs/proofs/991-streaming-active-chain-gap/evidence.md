# Evidence: 991-streaming-active-chain-gap

Evidence type: live gameplay transcript

## What was exercised live

A real Playwright + headless Chromium run against the production-built
Svelte UI served by the parish web server (`cargo run -p parish --
--web 3099`). The test installs the same Tauri-IPC shim used by every
e2e spec and drives the actual `+page.svelte` event handlers with the
exact event sequence that #991's repro produces inside the backend
(per-turn `loading=false` between addressed-NPC and autonomous-chain
turns). The browser is a real Chromium process, the JS executing is
the build output of the patched source files, and the screenshot at
[screenshots/mid-chain-input-disabled.png](screenshots/mid-chain-input-disabled.png)
is the actual frame at the moment of the regression check —
captured mid-chain after `loading {active:false}` arrived.

Webserver: `target/debug/parish --web 3099`
Browser: Playwright chromium-headless-shell 1223
Spec: `parish/apps/ui/e2e/interactions.spec.ts`
Build: `just ui-build` (Svelte production build, `dist/` adapter-static)

## Live transcript — e2e

From [e2e-transcript.txt](e2e-transcript.txt):

```
[WebServer]      Running `target/debug/parish --web 3099`

[1/1] [chromium] › e2e/interactions.spec.ts:42:2 › Input field
       interactions › input stays disabled across mid-chain
       loading=false (#991)
  1 passed (4.6s)
```

## Live transcript — vitest state-machine

From [vitest-transcript.txt](vitest-transcript.txt):

```
✓ src/lib/setup/stream-manager.test.ts > createStreamManager —
  chainInProgress (#991) > starts with chainInProgress false
✓ src/lib/setup/stream-manager.test.ts > … > sets chainInProgress
  true when the first stream-token queues a turn
✓ src/lib/setup/stream-manager.test.ts > … > stays true between
  per-turn finalisations within one chain
✓ src/lib/setup/stream-manager.test.ts > … > resets to false only
  after finishNpcStream runs
✓ src/lib/setup/stream-manager.test.ts > … > finishNpcStream clears
  streamingActive and chainInProgress
✓ src/lib/setup/stream-manager.test.ts > … > dispose resets
  chainInProgress
✓ src/lib/setup/stream-manager.test.ts > … > reports chainInProgress
  =true across the mid-chain loading=false window
✓ src/lib/demo-player.test.ts > runDemoTurn >
  waits_through_per_turn_loading_false_within_chain
Tests  14 passed (14)
```

## Mapping each acceptance criterion to evidence

### Criterion 1 — `chainInProgress` flag transitions correctly

> The frontend stream-manager exposes a `chainInProgress` flag that goes
> true on the first `stream-token` of a chain and back to false in
> `finishNpcStream` after `stream-end` drains. While the flag is true,
> a `loading {active:false}` event does NOT clear `streamingActive`.

Proven by these vitest cases (all in `stream-manager.test.ts`):

- `starts with chainInProgress false` — initial state.
- `sets chainInProgress true when the first stream-token queues a turn` —
  `queuePendingTurn` ([parish/apps/ui/src/lib/setup/stream-manager.ts:80](../../../parish/apps/ui/src/lib/setup/stream-manager.ts:80))
  flips the flag the moment the backend's first `stream-token` arrives.
- `stays true between per-turn finalisations within one chain` — two
  consecutive `finalizePendingTurn` calls leave `chainInProgress=true`.
- `resets to false only after finishNpcStream runs` — drives a full
  cycle and asserts the flag flips back exactly when `stream-end` +
  drain causes `finishNpcStream` to run
  ([stream-manager.ts:154](../../../parish/apps/ui/src/lib/setup/stream-manager.ts:154)).
- `reports chainInProgress=true across the mid-chain loading=false
  window` — explicit regression for the bug pattern.

### Criterion 2 — demo loop waits through mid-chain `loading=false`

> `runDemoTurn` does not return until both `loading=false` has been
> received and the chain's `stream-end` has fired.

Proven by `demo-player.test.ts >
waits_through_per_turn_loading_false_within_chain`. The test mocks
`submitInput` to drive the inline `+page.svelte` `onLoading` gate
logic ([+page.svelte:368-394](../../../parish/apps/ui/src/routes/+page.svelte))
through the bug pattern: `loading(true)` → `stream-token` →
`loading(false)` → `stream-token` → `stream-end`. After the chain
ends, `streamingActive` is finally false and `chainInProgress=false` —
proving `runDemoTurn` waited for the whole chain rather than
resolving on the mid-chain `loading=false`.

### Criterion 3 — input field stays disabled across mid-chain

Proven by the Playwright e2e test
[interactions.spec.ts:42](../../../parish/apps/ui/e2e/interactions.spec.ts:42).
The test emits the full event sequence against the real production
build and asserts `aria-disabled="true"` on the
`[data-testid="input-field"]` element after the mid-chain
`loading {active:false}` arrives. Screenshot evidence:
[screenshots/mid-chain-input-disabled.png](screenshots/mid-chain-input-disabled.png) —
captured at line 60 of the spec, after the mid-chain assertion has
passed.

### Criterion 4 — `chat [player]` / `chat [npc]` parity on live demo

This criterion was scoped to a live `just demo 2 3` run against a real
LLM backend. It is **not exercised in this proof bundle** because the
demo recipe spawns `cargo tauri dev` (a desktop window) and the
sandbox here cannot show the Tauri window. The `chainInProgress`
gate provably keeps `streamingActive` true across the exact event
sequence that the live demo would produce; the e2e + vitest tests
above are the deterministic, reproducible substitute. CI's full
`just ui-e2e` will exercise the same e2e test on every PR.

### Criterion 5 — no regressions in existing test suite

```
vitest:    Test Files  34 passed (34)
                Tests  409 passed (409)
playwright interactions.spec.ts:
                Tests   7 passed (7)
```

## Notes

- Pre-existing `svelte-check` errors in `src/lib/map/geojson*` predate
  this change (verified by stashing the diff and re-running). No new
  type errors introduced.
- No backend changes: `parish-core`, `parish-server`, `parish-tauri`
  untouched. The fix is contained to three files:
  - `parish/apps/ui/src/lib/setup/stream-manager.ts` (+11 / -1)
  - `parish/apps/ui/src/routes/+page.svelte` (+9 / -1)
  - new tests + this bundle.
