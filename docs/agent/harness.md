# Harness Map

One-page index of every check, sensor, skill, and audit in this repository — the _machinery_ an agent (or contributor) interacts with as part of normal work. Everything here is referenced from `AGENTS.md` / `CLAUDE.md`; this page exists so you don't have to assemble the picture from a half-dozen separate docs.

The framing comes from OpenAI's [harness-engineering post](https://openai.com/index/harness-engineering/) — the scaffolding around a coding agent matters as much as the agent itself. Every sensor here has a single purpose: turn a recurring kind of mistake into something `cargo test` (or CI) catches automatically, with a self-correcting error message.

## When you... → the harness... → lives at

| When you...                                                                                          | The harness...                                            | Lives at                                                                                                                 |
| ---------------------------------------------------------------------------------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| Edit a doc that cites a path                                                                         | Rejects nonexistent paths in any backtick-quoted token    | `parish/scripts/check-doc-paths.sh` (CI: `docs-consistency`, local: `just check`)                                        |
| Edit `AGENTS.md`                                                                                     | `CLAUDE.md` follows automatically                         | `CLAUDE.md` is a symlink to `AGENTS.md`                                                                                  |
| Add a runtime dep (`axum`, `tauri`, etc.) to a leaf crate                                            | Test fails citing the rule                                | `parish/crates/parish-core/tests/architecture_fitness.rs` → `backend_agnostic_crates_do_not_pull_runtime_deps`           |
| Create a top-level module under `parish/crates/parish-engine/src/` that shadows one in `parish-core` | Test fails with the canonical fix (extend the leaf crate) | `architecture_fitness.rs` → `parish_engine_does_not_duplicate_parish_core_modules`                                       |
| Leave a `.rs` file behind after a refactor (no `mod` declaration anywhere)                           | Test fails listing the orphan(s)                          | `architecture_fitness.rs` → `no_orphaned_source_files`                                                                   |
| Make one per-turn dialogue path drift from the others (skip a `DialogueOccurred` publish, etc.)      | Parity golden test fails with a stream diff               | `parish/crates/parish-engine/tests/mode_parity.rs` (the shared seam is `parish_core::game_session::apply_dialogue_turn`) |
| Change anything that affects gameplay JSON output                                                    | Snapshot baseline test fails with a `live                 | baseline` diff window                                                                                                    | `parish/crates/parish-engine/tests/eval_baselines.rs` |
| Introduce an out-of-period word in a fixture                                                         | Rubric fails                                              | `eval_baselines.rs` → `rubric_anachronisms_are_empty`                                                                    |
| Accidentally return `Moved { minutes: 0 }` (frozen clock)                                            | Rubric fails                                              | `eval_baselines.rs` → `rubric_movement_minutes_are_positive`                                                             |
| Silently break the location-description renderer                                                     | Rubric fails                                              | `eval_baselines.rs` → `rubric_look_descriptions_are_non_empty`                                                           |
| Leave AI partial-completion markers in changed files                                                 | Witness scan fails                                        | `parish/justfile` -> `witness-scan` (gates `just check` and `just verify`)                                               |
| Open a PR with runtime, UI, gameplay, CI, harness, or agent-instruction changes but no proof         | Agent proof gate fails                                    | `parish/scripts/agent-check.sh` (CI: `agent-check`, local: `just agent-check`)                                           |
| Want to know which gameplay subsystems lack a fixture                                                | Read-only report                                          | `just harness-audit` → `parish/scripts/harness-audit.sh`                                                                 |

## Skills

Slash commands defined in `.agents/skills/` (with `.claude/skills` as the symlink). Full table in [skills.md](skills.md); the gameplay-feature ones, in the order they get used:

1. **`/task-start <task-id>`** — start here for any task. Writes acceptance criteria + a verification fixture (and, for a non-trivial feature, a design note + plan); stops for review. Scaffold once, redirect cheap.
2. **`/parish-engine prove <feature>`** — after implementing, drive the feature through the script harness and read the JSON critically. Required for any gameplay change.
3. **`/parish-engine rubric`** — sister to `prove`: deterministic snapshot-diff + structural rubrics over baselined fixtures. Cheaper than reading JSON; runs on every `cargo test`.
4. **`/parish-engine play [scenario]`** — autonomous play-test, exploration-style. (`/parish-engine` also covers `harness`, `demo`, `browser`, and `screenshot` modes.)
5. **`/check`** — both gate levels: `just check` (`agent-check + fmt + clippy + test + witness-scan + check-doc-paths`, pre-commit) and `just verify` (adds the full harness walkthrough, pre-push).

## Quality gates in order

```text
local:  just agent-check      # proof evidence + judge verdict + fast debt scan
        just check    # agent-check + fmt + clippy + test + witness-scan + check-doc-paths
        just verify   # check + game-test fixture sweep
        just baselines        # only after intentional gameplay output changes (UPDATE_BASELINES=1)
        just harness-audit    # read-only coverage report

CI:     agent-check           # proof evidence + judge verdict + fast debt scan
        rust-quality-gate     # fmt + clippy + test (the architecture-fitness tests run here)
        rust-multi-channel    # cargo check on stable + beta
        docs-consistency      # check-doc-paths
        game-harness          # every fixture in testing/fixtures/
        ui-quality + ui-e2e   # frontend
```

## Where the harness ends

These rules are still **convention only** — no test enforces them. If you find yourself working around them, that's a candidate for the next sensor:

- Tests with behavior changes — `AGENTS.md` §3
- Content-level proof quality beyond the committed judge verdict — `AGENTS.md` §4 and §10
- No unexplained `#[allow]` — `AGENTS.md` §5
- Feature flags for new engine/gameplay features — `AGENTS.md` §6
- Mode-parity _wiring_ (every IPC handler called from every entry point) — `AGENTS.md` §2 (the _dep-level_ part is enforced; the wiring part isn't). Note: per-turn _dialogue-event_ parity across the harness / live-loop / headless paths **is** now enforced by the parity golden test (`mode_parity.rs`) plus the shared `apply_dialogue_turn` seam — it is no longer convention only.

## Turning a recurring mistake into a sensor

The point of the harness (see the intro) is that a mistake should only cost a
human once. Two feeders into "the next sensor":

1. **LEARNINGS sweep.** When you read or append a `LEARNINGS.md` entry, ask:
   _can this be a `cargo test` or CI check?_ If a failure mode is mechanically
   detectable (a struct-order rule, a "this code path must call X" rule, an
   anachronism, a banned token), it belongs as a fitness/rubric test, not as
   prose a future agent has to remember. Prefer a self-correcting failure
   message that names the fix. Entries that resist automation (judgement calls,
   environment quirks) stay as prose.
2. **Demo-audit findings.** Before closing a TODO.md finding in the
   `/todo-drain` loop, decide whether the _category_ warrants a permanent guard.
   A finding that has now been fixed more than once (e.g. auto-player movement,
   mid-conversation farewells, mood→emoji sign) is a rubric candidate — add the
   `rubric_*` test in `parish-engine/tests/eval_baselines.rs` in the same PR so
   the regression cannot silently return. A one-off content tweak does not.

If you find yourself working around any convention-only rule above, that is the
signal to promote it to a sensor.
