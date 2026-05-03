# Harness Map

One-page index of every check, sensor, skill, and audit in this repository — the *machinery* an agent (or contributor) interacts with as part of normal work. Everything here is referenced from `AGENTS.md` / `CLAUDE.md`; this page exists so you don't have to assemble the picture from a half-dozen separate docs.

The framing comes from OpenAI's [harness-engineering post](https://openai.com/index/harness-engineering/) — the scaffolding around a coding agent matters as much as the agent itself. Every sensor here has a single purpose: turn a recurring kind of mistake into something `cargo test` (or CI) catches automatically, with a self-correcting error message.

## When you... → the harness... → lives at

| When you... | The harness... | Lives at |
|---|---|---|
| Edit a doc that cites a path | Rejects nonexistent paths in any backtick-quoted token | `parish/scripts/check-doc-paths.sh` (CI: `docs-consistency`, local: `just check`) |
| Edit `AGENTS.md` | `CLAUDE.md` follows automatically | `CLAUDE.md` is a symlink to `AGENTS.md` |
| Add a runtime dep (`axum`, `tauri`, etc.) to a leaf crate | Test fails citing the rule | `parish/crates/parish-core/tests/architecture_fitness.rs` → `backend_agnostic_crates_do_not_pull_runtime_deps` |
| Create a top-level module under `parish/crates/parish-cli/src/` that shadows one in `parish-core` | Test fails with the canonical fix (extend the leaf crate) | `architecture_fitness.rs` → `parish_cli_does_not_duplicate_parish_core_modules` |
| Leave a `.rs` file behind after a refactor (no `mod` declaration anywhere) | Test fails listing the orphan(s) | `architecture_fitness.rs` → `no_orphaned_source_files` |
| Change anything that affects gameplay JSON output | Snapshot baseline test fails with a `live | baseline` diff window | `parish/crates/parish-cli/tests/eval_baselines.rs` |
| Introduce an out-of-period word in a fixture | Rubric fails | `eval_baselines.rs` → `rubric_anachronisms_are_empty` |
| Accidentally return `Moved { minutes: 0 }` (frozen clock) | Rubric fails | `eval_baselines.rs` → `rubric_movement_minutes_are_positive` |
| Silently break the location-description renderer | Rubric fails | `eval_baselines.rs` → `rubric_look_descriptions_are_non_empty` |
| Leave AI partial-completion markers in changed files | Witness scan fails | `parish/justfile` -> `witness-scan` (gates `just check` and `just verify`) |
| Open a PR with runtime, UI, gameplay, CI, harness, or agent-instruction changes but no proof | Agent proof gate fails | `parish/scripts/agent-check.sh` (CI: `agent-check`, local: `just agent-check`) |
| Want to know which gameplay subsystems lack a fixture | Read-only report | `just harness-audit` → `parish/scripts/harness-audit.sh` |

## Skills

Slash commands defined in `.agents/skills/` (with `.claude/skills` as the symlink). Full table in [skills.md](skills.md); the gameplay-feature ones, in the order they get used:

1. **`/feature-scaffold <name>`** — start here for a non-trivial feature. Generates a design note, a failing fixture, and a plan; stops for review. Scaffold once, redirect cheap.
2. **`/prove <feature>`** — after implementing, drive the feature through the script harness and read the JSON critically. Required for any gameplay change.
3. **`/rubric`** — sister to `/prove`: deterministic snapshot-diff + structural rubrics over baselined fixtures. Cheaper than reading JSON; runs on every `cargo test`.
4. **`/play [scenario]`** — autonomous play-test, exploration-style.
5. **`/check`** — `agent-check + fmt + clippy + test + witness-scan + check-doc-paths`. The pre-commit gate.
6. **`/verify`** — `/check` plus the full `game-test` walkthrough. The pre-push gate.

## Quality gates in order

```
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
- Mode-parity *wiring* (every IPC handler called from every entry point) — `AGENTS.md` §2 (the *dep-level* part is enforced; the wiring part isn't)
