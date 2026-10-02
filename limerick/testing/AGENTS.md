# limerick/testing — agent scope

Asserted scenarios, legacy harness fixtures, exploratory proof scripts, eval rubrics, and dialogue benchmark corpus. Central to proof-evidence gate ([truthful test automation](../../docs/agent/test-tooling-rules.md#truthful-test-automation)). See root [`AGENTS.md`](../../AGENTS.md), [`docs/agent/harness.md`](../../docs/agent/harness.md), the `/limerick-engine` skill (prove, play, rubric workflows), and [`limerick-mcp`](../crates/limerick-mcp/) for driving a live instance.

## Scoped commands

```sh
just test            # full workspace tests
just baselines       # regenerate harness baselines
just game-test       # walkthrough using fixtures/
just scenario-test   # asserted scenarios over limerick_core::game_loop
just agent-check     # proof-evidence gate
```

## Local gotchas

- **Integration-test cwd = crate root**, not workspace root. Fixture paths are `../../testing/fixtures/...` from `limerick/crates/<name>/`.
- **`scenarios/` is the regression format for new gameplay coverage.** Every YAML step runs through the shipping game loop and has a machine oracle. `fixtures/test_*.txt` is the legacy compatibility corpus.
- **`proofs/` scripts are evidence, not tests.** They use legacy harness syntax (one command per line, `#` comments) and are never swept as regressions merely because they exist.
- **`rundale-bench/` (repo root) corpus is append-only** so scores stay comparable: never edit existing prompts. The formal v1 freeze is still pending (see [`rundale-bench/AGENTS.md`](../../rundale-bench/AGENTS.md)). Use `/rundale-bench eval-dialogue` to compare new candidates.
- **`evals/` rubrics gate gameplay PRs.** Touching a rubric retroactively invalidates baselines — bump the version + note in PR.

## Layout

`scenarios/` asserted real-loop tests, `fixtures/` legacy regression scripts, `proofs/` one-off demonstrations, `evals/` rubric configs, `eval/` judge + player agent + rubrics + scenarios.
