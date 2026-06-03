# Design: harness-mock-shadow (Phase 0 + shadow scaffold)

## What changes

Today `GameTestHarness` (`parish-engine/src/testing.rs`) is a **second engine**:
it reimplements game-input routing, NPC dialogue, and system-command dispatch
in parallel to `parish_core::game_loop`. That parallel path is where harness
behavior drifts from the shipping engine (#985 absent-NPC, #1028 player-name) —
the #1159 theme.

The sustainable end state is a _strangler-fig_ consolidation: the harness keeps
its public API but internally drives the **real** `game_loop`, mocking only the
external boundary (the LLM). This task lands the scaffolding and a measurement,
nothing destructive:

1. mock the LLM at the existing `AnyClient` seam,
2. capture real-loop output via the real `EventEmitter` trait,
3. run **both** engines in lockstep over the whole corpus and record where they
   differ.

The legacy path remains the default and the oracle. The deliverable is the
**initial divergence ledger** — the go/no-go signal for Phases 1–4.

## Why a mock (not the simulator) and why here

`AnyClient::Simulator` already exists but emits Markov nonsense — uncontrolled,
so it can't back assertions, which is exactly why the harness grew its own
router above the seam. A _scriptable_ mock (`AnyClient::Mock`) makes both
engines deterministic pure functions of `(input, seeded RNG, scripted
completions) → events`. Determinism is the precondition for differential
testing; without it, lockstep comparison is impossible. Mocking at `AnyClient`
(not above the loop) is what lets the real dialogue path —
`prepare_npc_conversation → handle_npc_conversation → streaming` — execute
unchanged.

## Affected subsystems

- `parish-inference` — new `AnyClient::Mock(Arc<MockClient>)` variant +
  `MockClient` (scriptable queue, addressed-NPC matcher, deterministic empty
  fallback). Implements `generate` and `generate_stream` (tokens via the
  existing `mpsc::Sender<String>`). No new provider deps; never opens a socket.
- `parish-core` — no logic change. `CapturingEmitter` (impl of the existing
  `ipc::EventEmitter`) lives here as a reusable test double, next to the trait.
- `parish-engine` — `GameTestHarness::execute_via_real_loop`, the
  `normalize(events)` canonicalizer, and the `PARISH_HARNESS_SHADOW` lockstep
  wrapper around `execute`. The legacy `execute` body is untouched; shadow mode
  wraps it.
- CI / tooling — a non-gating `harness-shadow` job (+ `just harness-shadow`)
  that runs the corpus with the env set and uploads the ledger.

## Data model / seams

- `AnyClient::Mock` — additive enum variant; the `match` arms in `generate` /
  `generate_stream` (`parish-inference/src/lib.rs:759`, `:796`) gain a `Mock`
  arm. No change to callers.
- `CapturingEmitter` — `Mutex<Vec<(String, serde_json::Value)>>`; one method.
- `Canonical` — the normalized form compared in shadow mode. Normalization
  strips: timestamps, monotonic/log ids, and emit ordering for event classes
  that are semantically a set. Minimal and reviewed — over-normalization is the
  one real risk (it can mask a true diff), so a sample of cases is also run with
  normalization off to confirm residual diffs touch only the allowed fields.
- Divergence ledger — `target/harness-shadow-ledger.jsonl`, one JSON record per
  mismatch: `{case, input, old, new}`. Summarized into
  `docs/proofs/harness-shadow/initial-ledger.md`.

## Observable signal

Test-infrastructure, so the signal is `cargo test` + the ledger artifact, not a
gameplay JSON line. `execute_via_real_loop` reconstructs an
`ActionResult`-equivalent view from captured `text-log` / `world-update` events
(`parish-engine/src/testing.rs` `ActionResult`), which is what existing
assertions and the normalizer read.

## Feature flag

Gated by environment, not `config.flags`: `PARISH_HARNESS_SHADOW` is a
test-only switch with no shipping-runtime effect (the mock/emitter/real-loop
path is only constructed inside `GameTestHarness`). Default-off means the
existing suite and all baselines are byte-for-byte unaffected. A
`config.flags` entry would imply a production code path, which this is not.

## Non-goals (this task)

- Deleting the legacy router or making the real-loop path the default.
- Re-blessing any baseline.
- Touching the live model-quality evals — verified: no `GameTestHarness`
  reference under `parish/scripts/`, and `eval_baselines.rs` carries no
  ELO/`rundale-bench`/`eval-dialogue` wiring; those run the live server path.
- The architecture-fitness anti-regrowth rule (lands in Phase 4, after the
  legacy path is actually gone).
