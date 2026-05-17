Verdict: sufficient
Technical debt: clear
Acceptance criteria: met

The PR is a five-commit bundle landing six non-inference fixes from a 10-turn
`just demo` audit. Each fix is the smallest defensible change to the
behaviour it targets:

- `#13` and `#9` are wired through both the `parish-tauri` and `parish-server`
  paths so the mode-parity contract holds. The TOCTOU patch is a one-line
  guarded increment in each entry-point's tick task; no behaviour drift at
  the leaf-crate level.
- `#14` is a `c.is_simulator()` branch at the dispatch site in `parish-core`,
  with the existing canned-text fallback as the alternate path. A unit test
  drives `stream_reaction_texts` with a real `SimulatorClient` and asserts
  the canned line wins. The architectural fix (simulator-side category
  routing) is intentionally not attempted here — it belongs to the deferred
  inference-layer session.
- `#10` flips one boolean guard in `resolve_npc_targets`. Two new tests
  cover both the kept-fallback case (empty target list) and the new
  no-fallback case (target named but absent). The existing
  "no one here answers to that name" branch in `npc_turn.rs` now actually
  fires.
- `#11` is the cheapest defensible regression guard: a test that asserts
  every `GameMessage` produced by `apply_movement` has `source: "system"`.
  No code change ships with it; if the contract drifts in the future the
  test catches it at compile-time-adjacent.
- `#12` removes `snap.inference_paused` from the status-bar rAF freeze
  predicate. The "⏸ Paused" badge already used `snap.paused` alone, so the
  fix re-aligns the rAF behaviour with the visible indicator.

Verification: 2788/2788 cargo tests pass; 401/401 vitest pass; svelte-check
clean; `just demo 2 3` exits 0 with zero `World shifted` warnings (was 4 per
10 turns pre-fix) and zero simulator-corpus phrases leaking into chat
(`bridget`, `God help us`, `new collection`, `drainage situation`, etc.).

Deferred scope is explicit and documented in both the PR body and this
proof: JSON envelope leaks, anachronisms ("the famine" in an 1820 setting),
modern player register, hallucinated Gaelic, and blank-response handling
are inference-layer concerns assigned to a separate session per the user's
direction.

No new feature flags introduced; no new dependencies; README untouched
because the user-visible feature surface did not change.
