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
- **`rundale-bench/` (repo root) contains the pre-3.0 corpus and tooling.** Keep its historical corpus append-only when reproducing a requested legacy result (see [`rundale-bench/AGENTS.md`](../../rundale-bench/AGENTS.md)). Current model evaluations route to [dmooney/rundale-bench](https://github.com/dmooney/rundale-bench); read its current instructions and qualification evidence before running or publishing. Do not use `/rundale-bench eval-dialogue` for current requests.
- **`evals/` rubrics gate gameplay PRs.** Touching a rubric retroactively invalidates baselines — bump the version + note in PR.
- **`GameTestHarness::new()` and the `test_*.txt` fixtures play the large 1820 world in `limerick/testing/fixtures/mods/rundale-legacy/`, not `mods/rundale` (#2040).** Run those fixtures with `--game-mod testing/fixtures/mods/rundale-legacy`; tests of the shipped world use `GameTestHarness::new_with_mod_dir`.

## Layout

`scenarios/` asserted real-loop tests, `fixtures/` legacy regression scripts, `proofs/` one-off demonstrations, `evals/` rubric configs, `eval/` judge + player agent + rubrics + scenarios.
