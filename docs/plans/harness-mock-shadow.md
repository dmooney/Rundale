# Plan: harness-mock-shadow (Phase 0 + shadow scaffold)

Ordered, one logical change per commit. The whole sequence is additive — the
legacy harness path stays default and the existing suite must remain green at
every step (shadow mode off).

## Step 1 — Scriptable mock client (`parish-inference`)
- `feat(inference): scriptable AnyClient::Mock for deterministic tests`
- Add `mock_client/` (or `simulator/`-adjacent) `MockClient`: a `Mutex` queue of
  `(matcher, completion)` where matcher is "addressed NPC name or any". Methods
  mirror the simulator: `generate` returns the next matching completion (or a
  deterministic empty-queue fallback); `generate_stream` chunks that completion
  into the `mpsc::Sender<String>`.
- Add `AnyClient::Mock(Arc<MockClient>)` + `AnyClient::mock()` constructor; add
  `Mock` arms to `generate` / `generate_stream` matches.
- Unit tests → **C1**.

## Step 2 — Capturing emitter (`parish-core`)
- `test(core): CapturingEmitter test double for EventEmitter`
- `ipc/event_emitter.rs` (or a `testing` submodule): `CapturingEmitter` with
  `Mutex<Vec<(String, Value)>>`, `emit_event` push, and a `drain()`/`events()`
  accessor. `#[cfg(any(test, feature = "test-util"))]` or a plain `pub` test
  double per existing convention.
- Unit test → **C2**.

## Step 3 — Real-loop execution path (`parish-engine`)
- `feat(engine): GameTestHarness::execute_via_real_loop over game_loop`
- Build a `SystemCommandHost` / game-input context backed by the harness's
  existing `AppState`, the `CapturingEmitter`, and an injected `AnyClient::Mock`.
- `execute_via_real_loop(line)`: classify line (system vs game input) and call
  `parish_core::game_loop::handle_system_command` / `handle_game_input`; needs a
  `block_on` shim (current-thread runtime owned by the harness) since the loop is
  async and the harness is sync. Reconstruct an `ActionResult`-equivalent from
  captured events.
- Test: `look` through the real path → non-empty `text-log`/`world-update`
  describing the start location → **C3**.

## Step 4 — Normalizer (`parish-engine`)
- `feat(engine): normalize(events) canonical form for shadow comparison`
- `Canonical` + `normalize(&[(String, Value)]) -> Canonical`: drop timestamps /
  monotonic ids; sort set-semantic event classes; preserve order where order is
  semantic.
- Positive + negative unit tests → **C4**.

## Step 5 — Shadow mode wrapper (`parish-engine`)
- `feat(engine): PARISH_HARNESS_SHADOW lockstep + divergence ledger`
- Wrap `execute`: if env set, also run `execute_via_real_loop` on a cloned
  pre-state, normalize both, append `{case, input, old, new}` to
  `target/harness-shadow-ledger.jsonl` on mismatch. Case label from
  thread/test name or a harness-set tag. Env unset ⇒ untouched legacy path.
- Tests: forced match (silent) + forced divergence (one record); default-off
  produces no file → **C5**, **C7**.

## Step 6 — Corpus runner + CI job (tooling)
- `ci: non-gating harness-shadow job emitting the divergence ledger`
- `just harness-shadow`: `PARISH_HARNESS_SHADOW=1 cargo test -p parish-engine
  -p parish-core` (covers all five files), then summarize the JSONL into
  `docs/proofs/harness-shadow/initial-ledger.md`.
- `.github/workflows/…`: new job, `continue-on-error: true` (non-gating),
  uploads the ledger artifact.
- Run it for real; commit `initial-ledger.md` → **C6**.

## Step 7 — Verify the guards held
- `cargo test -p parish-engine -p parish-core -p parish-inference` green with env
  unset → **C7**; `cargo test -p parish-core --test architecture_fitness` green
  → **C8**.

## Tests to add/update
- New: mock-client unit (C1), capturing-emitter unit (C2), real-loop `look`
  (C3), normalizer ±(C4), shadow match/divergence/default-off (C5/C7).
- No existing baseline changes — if any move, that is a bug in this scaffold,
  not an expected edit.

## Proof
- Not a gameplay feature; proof bundle = this AC + the cargo-test transcript +
  the committed `initial-ledger.md` + judge mapping each criterion. A live
  gameplay transcript is not required (no runtime-shipping path changes; the
  mock/shadow code is test-only), but the shadow run over the corpus *is* the
  exercise that produces the ledger.
