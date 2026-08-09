# parish/testing — agent scope

Asserted scenarios, legacy harness fixtures, exploratory proof scripts, eval rubrics, and dialogue benchmark corpus. Central to proof-evidence gate (rule #10). See root [`AGENTS.md`](../../AGENTS.md), [`docs/agent/harness.md`](../../docs/agent/harness.md), and the `/prove` / `/rubric` / `/play` skills.

## Scoped commands

```sh
just test            # full workspace tests
just baselines       # regenerate harness baselines
just game-test       # walkthrough using fixtures/
just scenario-test   # asserted scenarios over parish_core::game_loop
just agent-check     # proof-evidence + judge verdict gate
```

## Local gotchas

- **Integration-test cwd = crate root**, not workspace root. Fixture paths are `../../testing/fixtures/...` from `parish/crates/<name>/`.
- **`scenarios/` is the regression format for new gameplay coverage.** Every YAML step runs through the shipping game loop and has a machine oracle. `fixtures/test_*.txt` is the legacy compatibility corpus.
- **`proofs/` scripts are evidence, not tests.** They use legacy harness syntax (one command per line, `#` comments) and are never swept as regressions merely because they exist.
- **`rundale-bench/` (repo root, `../../rundale-bench/`) Phase 1 corpus is frozen** for ELO comparability. Append-only — never edit existing prompts. Use `/eval-dialogue` to score new candidates.
- **`evals/` rubrics gate gameplay PRs.** Touching a rubric retroactively invalidates baselines — bump the version + note in PR.
- **Proof-bundle judge.md must be independent.** A judge written by the same agent that wrote the proof = no signal (rule #10).

## Layout

`scenarios/` asserted real-loop tests, `fixtures/` legacy regression scripts, `proofs/` one-off demonstrations, `evals/` rubric configs, `eval/` judge + player agent + rubrics + scenarios.
